//! Ships: a scene of her own, posed on the sea.
//!
//! A [`Craft`] owns a [`Scene`] whose grid is the ship in her own frame, bow
//! toward -y (up the screen), and a pose on the [`Sea`]: position, heading,
//! heave, pitch and roll. Her grid never turns; only the pose does. So her
//! decks, holds and people are ordinary cells that the rest of the engine
//! (fluid, fire, light, creatures, the view) handles unchanged, and a
//! bow-up view of her never shimmers as she comes about.
//!
//! **Building one.** Draw her in a `World`: hull sides and bottom as walls,
//! decks as floors, holds as open space inside the hull, with a margin of
//! open cells (at least 2) all round and below the keel. [`Craft::new`]
//! finds her hull envelope (every cell, level by level, that the outside
//! cannot reach), her waterplane and her draft for the given mass, and adds
//! the hull's lateral plane as a keel. Add sails and a rudder as [`Foil`]s.
//!
//! **Each tick** ([`Craft::step`]):
//! - *Motion* in sub-steps of 0.1 s. Every foil (sail, rudder, keel) feels
//!   the flow across it, air or water, relative to its own motion (so a
//!   turning ship's rudder sees the swing); it gives a normal force with a
//!   smooth stall and a little friction. The hull drags through the water
//!   with skin friction and wave-making resistance that climbs steeply past
//!   hull speed. She surges, sways (with the water she drags with her) and
//!   yaws. Heave, pitch and roll follow the sea surface sampled across her,
//!   as damped oscillators with periods from her size and metacentric
//!   height, so a beam sea near her roll period makes her roll heavily. Wind
//!   on her canvas heels her. The current carries her.
//! - *Grounding* where her keel would go below the sea bed.
//! - *Coupling* to her grid: the open columns outside her hull are filled to
//!   the sea surface where they are (so the sea stands along her side,
//!   rises and falls with the waves, and pours in over a low rail); shot
//!   holes leak by Torricelli's law between the water heights either side;
//!   the water inside her is weighed, and she settles deeper for it, until
//!   she founders.
//! - Her scene ticks (fire, smoke, flooding, her people), with the wind she
//!   feels (apparent wind, in her own frame).

use crate::ballistics::{Body, Damage, Placement, ShotEvent, V3};
use crate::color::Rgb;
use crate::frame::{Cell, Frame};
use crate::geom::{Pos, Rect, DIRS4};
use crate::material::MatId;
use crate::scene::Scene;
use crate::sea::{screen_axes, Sea, G, RHO_AIR, RHO_SEA};
use crate::things::Layer;
use crate::tile::Tile;
use crate::view::MapView;
use crate::world::FULL;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medium {
    Air,
    Water,
}

/// A surface that turns a flow into a force: a sail, a rudder, a keel.
#[derive(Clone, Debug)]
pub struct Foil {
    pub name: String,
    pub medium: Medium,
    /// Centre of effort in her grid's coordinates (cells: x across, y along
    /// toward the stern, z up the levels).
    pub at: V3,
    /// Area at full set, m^2.
    pub area: f32,
    /// Angle of the chord from fore-and-aft (radians, positive swings the
    /// leading edge to starboard): the rudder's angle, a yard's brace (a
    /// yard braced square is PI/2), a boom's sheet.
    pub angle: f32,
    /// How much of it is set, 0..1 (reefed sails, furled 0).
    pub set: f32,
    /// Normal-force slope per radian at small angles, and the normal-force
    /// coefficient broadside on.
    pub slope: f32,
    pub cn_max: f32,
    /// Friction along the chord.
    pub ct: f32,
}

impl Foil {
    /// A square sail: its chord along the yard, braced square by default.
    pub fn square_sail(name: &str, at: V3, area: f32) -> Foil {
        Foil { name: name.into(), medium: Medium::Air, at, area, angle: PI / 2.0, set: 1.0, slope: 3.0, cn_max: 1.3, ct: 0.05 }
    }

    /// A fore-and-aft sail (jib, spanker, lateen), sheeted along the centreline.
    pub fn fore_and_aft(name: &str, at: V3, area: f32) -> Foil {
        Foil { name: name.into(), medium: Medium::Air, at, area, angle: 0.0, set: 1.0, slope: 4.5, cn_max: 1.4, ct: 0.03 }
    }

    pub fn rudder(at: V3, area: f32) -> Foil {
        Foil { name: "rudder".into(), medium: Medium::Water, at, area, angle: 0.0, set: 1.0, slope: 3.5, cn_max: 1.2, ct: 0.01 }
    }

    /// A hull's lateral plane (added by [`Craft::new`]).
    pub fn keel(at: V3, area: f32) -> Foil {
        Foil { name: "keel".into(), medium: Medium::Water, at, area, angle: 0.0, set: 1.0, slope: 1.6, cn_max: 1.0, ct: 0.0 }
    }

    /// Normal-force coefficient for |sin(angle of attack)| = `s`: rises with
    /// `slope` at small angles and rounds over smoothly (no hard stall) to
    /// `cn_max` broadside on.
    #[inline]
    fn cn(&self, s: f32) -> f32 {
        let c = self.cn_max / (self.slope - self.cn_max).max(0.05);
        self.cn_max * s * (1.0 + c) / (s + c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floating {
    Afloat,
    /// Her keel is on the bottom somewhere.
    Aground,
    /// Too much water in her to float: she is going down.
    Foundered,
}

/// A hole between two cells (a shot hole through a plank) that water runs
/// through by the difference in its height either side.
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub wall: Pos,
    pub a: Pos,
    pub b: Pos,
    /// m^2.
    pub area: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum CraftEvent {
    /// She touched bottom at this speed (m/s).
    Grounded { speed: f32, at: (f32, f32) },
    Refloated,
    /// She ran into another craft (index in the slice given to [`collide`]).
    Collided { other: usize, impulse: f32, at: (f32, f32) },
    Foundered,
    /// A shot left a hole that leaks.
    Holed { wall: Pos, below_water: bool },
}

pub struct HullSpec {
    /// Her mass with everything aboard (kg), not counting water in her.
    pub mass: f32,
    /// Metacentric height (m): her stiffness against heeling.
    pub gm: f32,
    /// The sea water material (what the sea outside her is filled with).
    pub water: MatId,
    /// Multiplies hull resistance: 1 clean copper, 1.5 a foul bottom.
    pub fouling: f32,
}

pub struct Craft {
    pub name: String,
    pub scene: Scene,
    /// Sea-space position of her pivot (m), and its height (heave).
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Compass heading (radians, 0 north, clockwise).
    pub heading: f32,
    /// Velocity over the ground (m/s) and rate of turn (rad/s, to starboard).
    pub vx: f32,
    pub vy: f32,
    pub yaw_rate: f32,
    pub heave_v: f32,
    /// Bow up (radians) and its rate.
    pub pitch: f32,
    pub pitch_v: f32,
    /// Starboard down (radians) and its rate.
    pub roll: f32,
    pub roll_v: f32,
    pub foils: Vec<Foil>,
    pub mass: f32,
    pub gm: f32,
    pub water: MatId,
    pub fouling: f32,
    /// Sea water in her (kg).
    pub water_inside: f32,
    /// Waterline height in her grid (levels from the bottom of the grid).
    pub waterline: f32,
    pub keel_z: i32,
    /// Waterline length and beam (m), wetted area (m^2), displaced volume when
    /// built (m^3) and the volume of her whole envelope (m^3).
    pub length: f32,
    pub beam: f32,
    pub wetted: f32,
    pub envelope_volume: f32,
    /// Her pivot in grid coordinates (centre of her waterplane; z is the waterline).
    pub pivot: (f32, f32),
    pub state: Floating,
    pub holes: Vec<Hole>,
    pub damage: Damage,
    pub events: Vec<CraftEvent>,
    /// Natural periods (s) of heave/pitch and roll.
    pub heave_period: f32,
    pub roll_period: f32,
    area_by_level: Vec<f32>,
    /// Per cell: inside her hull envelope.
    envelope: Vec<bool>,
    /// Per column: inside her footprint (any level of envelope).
    footprint: Vec<bool>,
    sea_cols: Vec<(i32, i32, i32)>,
    /// Per column, the lowest level of her hull (the grid height if none).
    col_keel: Vec<i32>,
    hull_cells: Vec<usize>,
    /// Points along her keel for grounding (grid coordinates).
    keel_points: Vec<V3>,
    /// Waterline outline (grid coordinates) for collisions.
    outline: Vec<V3>,
    ahead: f32,
    clear_for: f32,
}

impl Craft {
    /// Float the ship drawn in `scene.world` (see the module notes) with her
    /// pivot at sea-space (x, y), heading `heading`.
    pub fn new(name: &str, scene: Scene, spec: HullSpec, x: f32, y: f32, heading: f32) -> Craft {
        let w = &scene.world;
        let (gw, gh, gd) = (w.w, w.h, w.d);
        let layer = w.layer();
        // The envelope: cells, level by level, that the outside cannot reach
        // through open cells.
        let mut envelope = vec![true; w.len()];
        let mut stack = Vec::new();
        for z in 0..gd {
            for y in 0..gh {
                for x in 0..gw {
                    if x == 0 || y == 0 || x == gw - 1 || y == gh - 1 {
                        let i = w.idx(x, y, z);
                        if !w.tile[i].is_wall() && envelope[i] {
                            envelope[i] = false;
                            stack.push(i);
                        }
                    }
                }
            }
            while let Some(i) = stack.pop() {
                let r = i % layer;
                let (cx, cy) = ((r % gw as usize) as i32, (r / gw as usize) as i32);
                for (dx, dy) in DIRS4 {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if nx < 0 || ny < 0 || nx >= gw || ny >= gh {
                        continue;
                    }
                    let j = w.idx(nx, ny, z);
                    if envelope[j] && !w.tile[j].is_wall() {
                        envelope[j] = false;
                        stack.push(j);
                    }
                }
            }
        }
        // Anything that is wall in the grid's border is not part of her.
        let mut footprint = vec![false; (gw * gh) as usize];
        let mut area_by_level = vec![0.0f32; gd as usize];
        let mut keel_z = gd;
        for z in 0..gd {
            for y in 0..gh {
                for x in 0..gw {
                    let i = w.idx(x, y, z);
                    if envelope[i] {
                        footprint[(y * gw + x) as usize] = true;
                        area_by_level[z as usize] += 1.0;
                        keel_z = keel_z.min(z);
                    }
                }
            }
        }
        assert!(keel_z < gd, "craft {name}: no hull found (draw her sides as walls inside an open margin)");
        let envelope_volume: f32 = area_by_level.iter().sum();
        let waterline = level_for_volume(&area_by_level, keel_z, spec.mass / RHO_SEA);
        let wl_z = (waterline.floor() as i32).clamp(keel_z, gd - 1);
        // Waterplane centroid and extent.
        let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0.0f32);
        let (mut x0, mut x1, mut y0, mut y1) = (gw, 0, gh, 0);
        for y in 0..gh {
            for x in 0..gw {
                if envelope[w.idx(x, y, wl_z)] {
                    sx += x as f32 + 0.5;
                    sy += y as f32 + 0.5;
                    n += 1.0;
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        let pivot = (sx / n.max(1.0), sy / n.max(1.0));
        let length = (y1 - y0 + 1) as f32;
        let beam = (x1 - x0 + 1) as f32;
        let draft = (waterline - keel_z as f32).max(0.3);
        let wetted = length * (beam + 2.0 * draft) * 0.7;
        // Per column, the lowest level of her hull: below it is the sea
        // (under her keel, or the whole column outside her).
        let mut col_keel = vec![gd; (gw * gh) as usize];
        let mut sea_cols = Vec::new();
        let mut hull_cells = Vec::new();
        for y in 0..gh {
            for x in 0..gw {
                let k = (0..gd).find(|&z| envelope[w.idx(x, y, z)]).unwrap_or(gd);
                col_keel[(y * gw + x) as usize] = k;
                if k > 0 {
                    sea_cols.push((x, y, k));
                }
                for z in k..gd {
                    hull_cells.push(w.idx(x, y, z));
                }
            }
        }
        // Keel points: along the bottom of the footprint at bow, stern and
        // the turn of the bilge either side.
        let kz = keel_z as f32;
        let (cx, cy) = pivot;
        let keel_points = vec![
            [cx, y0 as f32 + 0.5, kz],
            [cx, y1 as f32 + 0.5, kz],
            [cx, cy, kz],
            [x0 as f32 + 0.5, cy, kz + 0.5],
            [x1 as f32 + 0.5, cy, kz + 0.5],
            [cx, y0 as f32 + 0.25 * (y1 - y0) as f32 + 0.5, kz],
            [cx, y0 as f32 + 0.75 * (y1 - y0) as f32 + 0.5, kz],
        ];
        let mut outline = Vec::new();
        for y in 0..gh {
            for x in 0..gw {
                let i = w.idx(x, y, wl_z);
                if !envelope[i] {
                    continue;
                }
                let edge = DIRS4.iter().any(|(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx < 0 || ny < 0 || nx >= gw || ny >= gh || !envelope[w.idx(nx, ny, wl_z)]
                });
                if edge {
                    outline.push([x as f32 + 0.5, y as f32 + 0.5, waterline]);
                }
            }
        }
        let gm = spec.gm.max(0.05);
        let roll_period = 0.8 * beam / gm.sqrt();
        let heave_period = TAU * ((spec.mass / RHO_SEA) / (n.max(1.0)) / G).sqrt();
        let mut foils = Vec::new();
        foils.push(Foil::keel([cx, cy, kz + draft * 0.5], length * draft * 0.75));
        let mut c = Craft {
            name: name.into(),
            scene,
            x,
            y,
            z: 0.0,
            heading,
            vx: 0.0,
            vy: 0.0,
            yaw_rate: 0.0,
            heave_v: 0.0,
            pitch: 0.0,
            pitch_v: 0.0,
            roll: 0.0,
            roll_v: 0.0,
            foils,
            mass: spec.mass,
            gm,
            water: spec.water,
            fouling: spec.fouling.max(0.1),
            water_inside: 0.0,
            waterline,
            keel_z,
            length,
            beam,
            wetted,
            envelope_volume,
            pivot,
            state: Floating::Afloat,
            holes: Vec::new(),
            damage: Damage::default(),
            events: Vec::new(),
            heave_period,
            roll_period,
            area_by_level,
            envelope,
            footprint,
            sea_cols,
            col_keel,
            hull_cells,
            keel_points,
            outline,
            ahead: 0.0,
            clear_for: 0.0,
        };
        let _ = layer;
        c.scene.world.wake.wake_all();
        c
    }

    /// Unit vectors (sea space) of her bow and her starboard side.
    pub fn axes(&self) -> ((f32, f32), (f32, f32)) {
        let (s, c) = self.heading.sin_cos();
        ((s, -c), (c, s))
    }

    /// How her grid sits in sea space right now.
    pub fn placement(&self) -> Placement {
        Placement::pose([self.x, self.y, self.z], [self.pivot.0, self.pivot.1, self.waterline], self.heading, self.pitch, self.roll)
    }

    /// Lend her grid to the ballistics as body `id`.
    pub fn body(&mut self, id: usize) -> Body<'_> {
        let place = self.placement();
        Body { id, world: &mut self.scene.world, things: &mut self.scene.things, place, damage: &mut self.damage }
    }

    /// Speed over the ground (m/s).
    pub fn speed(&self) -> f32 {
        (self.vx * self.vx + self.vy * self.vy).sqrt()
    }

    /// Draft (m from keel to waterline).
    pub fn draft(&self) -> f32 {
        self.waterline - self.keel_z as f32
    }

    /// Whether a grid cell is part of her hull envelope.
    pub fn in_hull(&self, p: Pos) -> bool {
        self.scene.world.index(p).is_some_and(|i| self.envelope[i])
    }

    /// Whether a grid column lies inside her footprint.
    pub fn in_footprint(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.scene.world.w && y < self.scene.world.h && self.footprint[(y * self.scene.world.w + x) as usize]
    }

    /// The foil with this name, to trim it.
    pub fn foil_mut(&mut self, name: &str) -> Option<&mut Foil> {
        self.foils.iter_mut().find(|f| f.name == name)
    }

    /// Put the helm over: the rudder to `angle` radians (positive turns her to starboard).
    pub fn helm(&mut self, angle: f32) {
        if let Some(r) = self.foil_mut("rudder") {
            // The rudder's trailing edge goes the other way from the turn.
            r.angle = -angle.clamp(-0.6, 0.6);
        }
    }

    /// Advance her one tick of her scene's clock under `wind` (m/s, the way
    /// it blows). Step the sea yourself (once per tick, before the ships).
    pub fn step(&mut self, sea: &Sea, wind: (f32, f32)) {
        let dt = self.scene.clock.secs_per_tick as f32;
        let n = (dt / 0.1).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        self.ahead = -dt;
        for _ in 0..n {
            self.ahead += h;
            self.motion(h, sea, wind);
        }
        self.ahead = 0.0;
        self.couple(sea, dt);
        // The wind she feels, in her own frame (cells per tick).
        let (fwd, stb) = self.axes();
        let (ax, ay) = (wind.0 - self.vx, wind.1 - self.vy);
        let (lx, ly) = (ax * stb.0 + ay * stb.1, -(ax * fwd.0 + ay * fwd.1));
        let sp = (lx * lx + ly * ly).sqrt() * dt;
        self.scene.weather.set_wind(sp, ly.atan2(lx));
        self.scene.tick();
    }

    fn sample(&self, sea: &Sea, a: f32, s: f32) -> f32 {
        let (fwd, stb) = self.axes();
        sea.height_in(self.x + fwd.0 * a + stb.0 * s, self.y + fwd.1 * a + stb.1 * s, self.ahead)
    }

    fn motion(&mut self, h: f32, sea: &Sea, wind: (f32, f32)) {
        let (fwd, stb) = self.axes();
        let (l, b) = (self.length, self.beam);
        let m = self.mass + self.water_inside;
        let draft = self.draft();

        // Vertical: heave, pitch and roll toward the sea under her.
        let (hb, hs, hp, hst, hc) = (
            self.sample(sea, l * 0.45, 0.0),
            self.sample(sea, -l * 0.45, 0.0),
            self.sample(sea, 0.0, -b * 0.45),
            self.sample(sea, 0.0, b * 0.45),
            self.sample(sea, 0.0, 0.0),
        );
        let z_eq = (hb + hs + hp + hst + 2.0 * hc) / 6.0;
        let wh = TAU / self.heave_period.max(1.0);
        if self.state == Floating::Foundered {
            self.heave_v = (self.heave_v - 0.5 * h).max(-0.6);
        } else {
            self.heave_v += (wh * wh * (z_eq - self.z) - 2.0 * 0.3 * wh * self.heave_v) * h;
        }
        self.z += self.heave_v * h;
        let pitch_eq = ((hb - hs) / (0.9 * l)).atan();
        let wp = TAU / (self.heave_period * 0.9).max(1.0);
        self.pitch_v += (wp * wp * (pitch_eq - self.pitch) - 2.0 * 0.4 * wp * self.pitch_v) * h;
        self.pitch += self.pitch_v * h;

        // Horizontal forces (sea space) and moments.
        let (mut fx, mut fy, mut yaw_m, mut heel_m) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let cur = sea.current(self.x, self.y);
        let heel_loss = self.roll.cos().max(0.0);
        for f in &self.foils {
            let a = self.pivot.1 - f.at[1]; // forward of the pivot
            let s = f.at[0] - self.pivot.0; // to starboard
            let r = self.yaw_rate;
            let vp = (self.vx + r * (a * stb.0 - s * fwd.0), self.vy + r * (a * stb.1 - s * fwd.1));
            let (fluid, rho) = match f.medium {
                Medium::Air => (wind, RHO_AIR),
                Medium::Water => (cur, RHO_SEA),
            };
            let w = (fluid.0 - vp.0, fluid.1 - vp.1);
            let wv = (w.0 * w.0 + w.1 * w.1).sqrt();
            if wv < 1e-4 || f.set <= 0.0 {
                continue;
            }
            let (sa, ca) = f.angle.sin_cos();
            let chord = (fwd.0 * ca + stb.0 * sa, fwd.1 * ca + stb.1 * sa);
            let normal = (stb.0 * ca - fwd.0 * sa, stb.1 * ca - fwd.1 * sa);
            let wn = w.0 * normal.0 + w.1 * normal.1;
            let wc = w.0 * chord.0 + w.1 * chord.1;
            let q = 0.5 * rho * wv * wv * f.area * f.set * if f.medium == Medium::Air { heel_loss } else { 1.0 };
            let cn = f.cn(wn.abs() / wv) * wn.signum();
            let ct = f.ct * wc / wv;
            let force = (q * (cn * normal.0 + ct * chord.0), q * (cn * normal.1 + ct * chord.1));
            fx += force.0;
            fy += force.1;
            let side = force.0 * stb.0 + force.1 * stb.1;
            let along = force.0 * fwd.0 + force.1 * fwd.1;
            yaw_m += a * side - s * along;
            if f.medium == Medium::Air {
                heel_m += side * (f.at[2] - self.waterline + draft * 0.5);
            }
        }
        // Hull resistance through the water.
        let rel = (self.vx - cur.0, self.vy - cur.1);
        let u = rel.0 * fwd.0 + rel.1 * fwd.1;
        let fnum = u.abs() / (G * l).sqrt();
        let cw = 0.0015 * (fnum / 0.3).powi(4);
        // Skin friction by the ITTC-1957 line (with a form factor of 1.15):
        // relatively larger at low speeds, ~0.002 at a brig's best.
        let re = (u.abs() * l / 1.19e-6).max(1e5);
        let cf = 1.15 * 0.075 / (re.log10() - 2.0).powi(2);
        let res = 0.5 * RHO_SEA * self.wetted * u * u.abs() * (cf + cw) * self.fouling;
        fx -= fwd.0 * res;
        fy -= fwd.1 * res;
        // The hull resists turning: lateral drag along her length.
        let r = self.yaw_rate;
        yaw_m -= 0.5 * RHO_SEA * draft * l.powi(4) / 32.0 * r * r.abs() + 0.02 * m * l * r;

        // Surge and sway, with the water she drags sideways.
        let f_fwd = fx * fwd.0 + fy * fwd.1;
        let f_stb = fx * stb.0 + fy * stb.1;
        let (a_fwd, a_stb) = (f_fwd / (1.05 * m), f_stb / (1.8 * m));
        self.vx += (fwd.0 * a_fwd + stb.0 * a_stb) * h;
        self.vy += (fwd.1 * a_fwd + stb.1 * a_stb) * h;
        let inertia = m * (l * l + b * b) / 12.0 * 1.3;
        self.yaw_rate += yaw_m / inertia * h;
        self.heading = (self.heading + self.yaw_rate * h).rem_euclid(TAU);
        self.x += self.vx * h;
        self.y += self.vy * h;

        // Roll: the sea's slope across her, plus the wind's heel.
        let wave_roll = ((hp - hst) / (0.9 * b)).atan();
        let heel = (heel_m / (m * G * self.gm)).clamp(-1.2, 1.2);
        let wr = TAU / self.roll_period.max(1.0);
        self.roll_v += (wr * wr * (wave_roll + heel - self.roll) - 2.0 * 0.08 * wr * self.roll_v) * h;
        self.roll = (self.roll + self.roll_v * h).clamp(-1.4, 1.4);

        // The bottom.
        let place = self.placement();
        let mut worst = 0.0f32;
        let mut at = (self.x, self.y);
        for k in &self.keel_points {
            let p = place.to_world(*k);
            let bed = -sea.bed(p[0], p[1]);
            if p[2] < bed && bed - p[2] > worst {
                worst = bed - p[2];
                at = (p[0], p[1]);
            }
        }
        if worst > 0.0 {
            self.clear_for = 0.0;
            if self.state == Floating::Afloat {
                self.events.push(CraftEvent::Grounded { speed: self.speed(), at });
                self.state = Floating::Aground;
            }
            // She rests on the bottom and the ground holds her.
            self.z += worst;
            self.heave_v = self.heave_v.max(0.0);
            let k = (-3.0 * h).exp();
            self.vx *= k;
            self.vy *= k;
            self.yaw_rate *= k;
        } else if self.state == Floating::Aground {
            // Only call her afloat once she has been clear of the bottom a while,
            // or a ship pounding on a bank would float off every other wave.
            self.clear_for += h;
            if self.clear_for > 3.0 {
                self.state = Floating::Afloat;
                self.events.push(CraftEvent::Refloated);
            }
        }
    }

    /// The sea surface at a grid column, in her grid's levels.
    fn surface_level(&self, sea: &Sea, place: &Placement, x: f32, y: f32) -> f32 {
        let p = place.to_world([x, y, self.waterline]);
        let eta = sea.height(p[0], p[1]);
        let r = &place.r;
        let (dx, dy) = (x - self.pivot.0, y - self.pivot.1);
        self.waterline + (eta - place.t[2] - r[2][0] * dx - r[2][1] * dy) / r[2][2].max(0.1)
    }

    /// Head of water at a cell (levels): the sea's surface if the cell is
    /// outside her, else the top of the water standing over it.
    /// Whether a cell is open sea (outside her, or under her keel).
    fn is_sea(&self, p: Pos) -> bool {
        let w = &self.scene.world;
        if p.x < 0 || p.y < 0 || p.x >= w.w || p.y >= w.h {
            return true;
        }
        p.z < self.col_keel[(p.y * w.w + p.x) as usize]
    }

    fn head(&self, sea: &Sea, place: &Placement, p: Pos) -> f32 {
        if self.is_sea(p) {
            return self.surface_level(sea, place, p.x as f32 + 0.5, p.y as f32 + 0.5);
        }
        let w = &self.scene.world;
        let mut z = p.z;
        let mut head = p.z as f32;
        while z < w.d {
            let i = w.idx(p.x, p.y, z);
            let f = w.fluid[i];
            if f == 0 {
                break;
            }
            head = z as f32 + f as f32 / FULL as f32;
            if f < FULL {
                break;
            }
            z += 1;
        }
        head
    }

    fn couple(&mut self, sea: &Sea, dt: f32) {
        let place = self.placement();
        // The sea along her side.
        let cols = std::mem::take(&mut self.sea_cols);
        for &(x, y, top) in &cols {
            let ls = self.surface_level(sea, &place, x as f32 + 0.5, y as f32 + 0.5);
            let w = &mut self.scene.world;
            for z in 0..top {
                let i = w.idx(x, y, z);
                if w.tile[i].is_wall() {
                    continue;
                }
                let target = if (z + 1) as f32 <= ls {
                    FULL
                } else if (z as f32) < ls {
                    ((ls - z as f32) * FULL as f32) as u8
                } else {
                    0
                };
                let cur = w.fluid[i];
                if cur != target && (cur == 0 || w.fluid_mat[i] == self.water) {
                    w.fluid[i] = target;
                    w.fluid_mat[i] = if target > 0 { self.water } else { crate::material::AIR };
                    w.touch(i);
                }
            }
        }
        self.sea_cols = cols;

        // Holes leak.
        let holes = std::mem::take(&mut self.holes);
        let mut kept = Vec::with_capacity(holes.len());
        for hole in holes {
            if !self.scene.world.tile(hole.wall).is_wall() && !self.scene.world.tile(hole.wall).has_floor() {
                continue; // the plank is gone: the water goes straight through
            }
            let (ha, hb) = (self.head(sea, &place, hole.a), self.head(sea, &place, hole.b));
            let zh = hole.wall.z as f32 + 0.5;
            let (hi, lo, from, to) = if ha > hb { (ha, hb, hole.a, hole.b) } else { (hb, ha, hole.b, hole.a) };
            if hi > zh {
                let dh = hi - lo.max(zh);
                let q = 0.6 * hole.area * (2.0 * G * dh.max(0.0)).sqrt(); // m^3/s
                let units = (q * dt * FULL as f32).round().min(255.0) as u8;
                if units > 0 {
                    let water = self.water;
                    let from_sea = self.is_sea(from);
                    let w = &mut self.scene.world;
                    let moved = w.add_fluid(to, water, units);
                    if !from_sea {
                        w.take_fluid(from, moved);
                    }
                }
            }
            kept.push(hole);
        }
        self.holes = kept;

        // Weigh the water in her and settle to it.
        let w = &self.scene.world;
        let mut units = 0u64;
        for &i in &self.hull_cells {
            if w.fluid_mat[i] == self.water {
                units += w.fluid[i] as u64;
            }
        }
        self.water_inside = units as f32 / FULL as f32 * RHO_SEA;
        let need = (self.mass + self.water_inside) / RHO_SEA;
        if need >= self.envelope_volume * 0.98 {
            if self.state != Floating::Foundered {
                self.state = Floating::Foundered;
                self.events.push(CraftEvent::Foundered);
            }
        } else {
            let target = level_for_volume(&self.area_by_level, self.keel_z, need);
            self.waterline += (target - self.waterline) * (dt / 8.0).min(1.0);
        }
    }

    /// Record the holes her own hits left (pass every event; those for other
    /// bodies are ignored). Call after each ballistics step.
    pub fn take_hits(&mut self, id: usize, events: &[ShotEvent], sea: &Sea) {
        let place = self.placement();
        for e in events {
            if let ShotEvent::Struck { body, cell, through, broke, from, to, area, .. } = *e {
                if body != id || broke {
                    continue;
                }
                let (Some(a), Some(b)) = (from, to) else { continue };
                let below = (cell.z as f32) < self.surface_level(sea, &place, cell.x as f32 + 0.5, cell.y as f32 + 0.5);
                // A ball that went through leaves its own hole; one that lodged
                // below the waterline still springs the plank, a smaller leak.
                let area = if through {
                    area
                } else if below {
                    area * 0.4
                } else {
                    continue;
                };
                if let Some(hole) = self.holes.iter_mut().find(|h| h.wall == cell) {
                    hole.area += area;
                } else {
                    self.holes.push(Hole { wall: cell, a, b, area });
                }
                self.events.push(CraftEvent::Holed { wall: cell, below_water: below });
            }
        }
    }

    /// Stop a hole (the carpenter's plug).
    pub fn plug(&mut self, wall: Pos) {
        self.holes.retain(|h| h.wall != wall);
    }

    /// The level of her grid at sea-space height `z` near her pivot.
    pub fn level_at_height(&self, z: f32) -> i32 {
        (self.waterline + (z - self.z)).floor() as i32
    }

    /// What she looks like from above at a sea-space point, at or below
    /// sea-space height `z` (None if she is not there).
    pub fn top_look(&self, p: V3) -> Option<Cell> {
        let l = self.placement().to_local(p);
        let (x, y) = (l[0].floor() as i32, l[1].floor() as i32);
        if !self.in_footprint(x, y) {
            return None;
        }
        let w = &self.scene.world;
        let top = (l[2].floor() as i32).clamp(0, w.d - 1);
        for z in (0..=top).rev() {
            let pos = Pos::new(x, y, z);
            if let Some((_, t)) = self.scene.things.at(pos).max_by_key(|(_, t)| self.scene.things.kind_of(t).layer) {
                let k = self.scene.things.kind_of(t);
                if k.layer >= Layer::Furniture {
                    return Some(Cell::new(k.glyph, t.color.unwrap_or(k.fg), Rgb(40, 30, 20)));
                }
            }
            let i = w.idx(x, y, z);
            let m = w.mats.get(w.mat[i]);
            match w.tile[i] {
                Tile::Open => continue,
                Tile::Wall => return Some(Cell::new(m.wall_glyph, m.fg, m.bg)),
                _ => return Some(Cell::new(m.floor_glyphs.first().copied().unwrap_or('.'), m.fg, m.bg)),
            }
        }
        None
    }
}

/// Level (from the grid bottom) at which a hull with these areas per level
/// displaces `vol` m^3.
fn level_for_volume(area: &[f32], keel: i32, vol: f32) -> f32 {
    let mut left = vol;
    for (z, &a) in area.iter().enumerate().skip(keel.max(0) as usize) {
        if a <= 0.0 {
            continue;
        }
        if left <= a {
            return z as f32 + left / a;
        }
        left -= a;
    }
    area.len() as f32
}

/// Push apart crafts whose hulls overlap, with an inelastic knock, and
/// report it in both crafts' events.
pub fn collide(crafts: &mut [Craft]) {
    let n = crafts.len();
    for i in 0..n {
        for j in i + 1..n {
            let (left, right) = crafts.split_at_mut(j);
            let (a, b) = (&mut left[i], &mut right[0]);
            let reach = 0.5 * (a.length.max(a.beam) + b.length.max(b.beam));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            if dx * dx + dy * dy > reach * reach {
                continue;
            }
            let (pa, pb) = (a.placement(), b.placement());
            let (mut cx, mut cy, mut k) = (0.0, 0.0, 0.0);
            for p in &a.outline {
                let w = pa.to_world(*p);
                let l = pb.to_local(w);
                if b.in_footprint(l[0].floor() as i32, l[1].floor() as i32) {
                    cx += w[0];
                    cy += w[1];
                    k += 1.0;
                }
            }
            if k == 0.0 {
                continue;
            }
            let at = (cx / k, cy / k);
            let d = (dx * dx + dy * dy).sqrt().max(0.1);
            let nrm = (dx / d, dy / d);
            let rel = (b.vx - a.vx) * nrm.0 + (b.vy - a.vy) * nrm.1;
            let (ma, mb) = (a.mass + a.water_inside, b.mass + b.water_inside);
            let mut impulse = 0.0;
            if rel < 0.0 {
                impulse = -(1.0 + 0.1) * rel / (1.0 / ma + 1.0 / mb);
                a.vx -= impulse / ma * nrm.0;
                a.vy -= impulse / ma * nrm.1;
                b.vx += impulse / mb * nrm.0;
                b.vy += impulse / mb * nrm.1;
                // A glancing blow turns them.
                for (c, sign, m) in [(&mut *a, -1.0f32, ma), (&mut *b, 1.0, mb)] {
                    let (fwd, stb) = c.axes();
                    let (ox, oy) = (at.0 - c.x, at.1 - c.y);
                    let (aa, ss) = (ox * fwd.0 + oy * fwd.1, ox * stb.0 + oy * stb.1);
                    let (jx, jy) = (sign * impulse * nrm.0, sign * impulse * nrm.1);
                    let tq = aa * (jx * stb.0 + jy * stb.1) - ss * (jx * fwd.0 + jy * fwd.1);
                    c.yaw_rate += tq / (m * (c.length * c.length + c.beam * c.beam) / 12.0 * 1.3);
                }
            }
            // Separate them a little each tick until clear.
            let push = 0.05 * k.min(10.0);
            a.x -= nrm.0 * push * mb / (ma + mb);
            a.y -= nrm.1 * push * mb / (ma + mb);
            b.x += nrm.0 * push * ma / (ma + mb);
            b.y += nrm.1 * push * ma / (ma + mb);
            a.events.push(CraftEvent::Collided { other: j, impulse, at });
            b.events.push(CraftEvent::Collided { other: i, impulse, at });
        }
    }
}

/// Draw `craft` with `view` (a normal map view of her grid, bow up), then
/// the sea and any `others` around and alongside her, sampled in sea space
/// where her grid ends.
pub fn draw(frame: &mut Frame, area: Rect, view: &mut MapView, craft: &Craft, sea: &Sea, others: &[&Craft]) {
    view.draw(&craft.scene, frame, area);
    let (cols, rows) = view.cells_for(area);
    let (x0, y0) = (view.center.x - cols / 2, view.center.y - rows / 2);
    let w = &craft.scene.world;
    let z = view.center.z.clamp(0, w.d - 1);
    let place = craft.placement();
    for sy in 0..rows {
        for sx in 0..cols {
            let (wx, wy) = (x0 + sx, y0 + sy);
            let inside = wx >= 0 && wy >= 0 && wx < w.w && wy < w.h;
            if inside && craft.in_footprint(wx, wy) {
                continue;
            }
            // Her margin of simulated sea is drawn like the open sea beyond it,
            // unless something is in it there (a man overboard, wreckage).
            let busy = inside && (0..=z).any(|zz| craft.scene.things.has_any_at(Pos::new(wx, wy, zz)));
            if busy {
                continue;
            }
            let p = place.to_world([wx as f32 + 0.5, wy as f32 + 0.5, z as f32 + 0.5]);
            let mut cell = others.iter().find_map(|o| o.top_look(p));
            if cell.is_none() {
                cell = Some(sea.look(p[0], p[1], 1.0));
            }
            let Some(c) = cell else { continue };
            let px = area.x + if view.wide { sx * 2 } else { sx };
            frame.set(px, area.y + sy, c);
            if view.wide {
                let c2 = if matches!(c.ch, '~' | '-' | '#' | '=' | '+') { c } else { Cell::new(' ', c.bg, c.bg) };
                frame.set(px + 1, area.y + sy, c2);
            }
        }
    }
}

/// A chart: the sea at `mpc` metres per cell around (cx, cy), compass `up`
/// up the screen, with each craft drawn as her waterline footprint when she
/// is big enough to show, else as an arrow along her heading.
#[allow(clippy::too_many_arguments)]
pub fn draw_chart(frame: &mut Frame, area: Rect, sea: &Sea, crafts: &[&Craft], cx: f32, cy: f32, mpc: f32, up: f32, wide: bool) {
    sea.draw(frame, area, cx, cy, mpc, up, wide);
    let cols = if wide { area.w / 2 } else { area.w };
    let (right, down) = screen_axes(up);
    let to_screen = |x: f32, y: f32| {
        let (rx, ry) = (x - cx, y - cy);
        let sx = cols / 2 + ((rx * right.0 + ry * right.1) / mpc).round() as i32;
        let sy = area.h / 2 + ((rx * down.0 + ry * down.1) / mpc).round() as i32;
        (sx, sy)
    };
    for c in crafts {
        let colour = Rgb(230, 210, 160);
        let mut put = |sx: i32, sy: i32, ch: char| {
            if sx >= 0 && sy >= 0 && sx < cols && sy < area.h {
                let px = area.x + if wide { sx * 2 } else { sx };
                let bg = frame.get(px, area.y + sy).bg;
                frame.set(px, area.y + sy, Cell::new(ch, colour, bg));
                if wide {
                    frame.set(px + 1, area.y + sy, Cell::new(if ch == '#' { '#' } else { ' ' }, colour, bg));
                }
            }
        };
        if c.length / mpc >= 3.0 {
            let place = c.placement();
            for p in &c.outline {
                let wpt = place.to_world(*p);
                let (sx, sy) = to_screen(wpt[0], wpt[1]);
                put(sx, sy, '#');
            }
        } else {
            let rel = (c.heading - up).rem_euclid(TAU);
            let k = ((rel / (PI / 4.0)).round() as usize) % 8;
            let (sx, sy) = to_screen(c.x, c.y);
            put(sx, sy, ['^', '/', '>', '\\', 'v', '/', '<', '\\'][k]);
        }
    }
}
