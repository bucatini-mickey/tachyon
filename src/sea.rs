//! The open sea, modelled as fields rather than cells.
//!
//! A per-cell liquid grid is right for a canal or a flooded hold, but not for
//! miles of open water. The sea here is a handful of smooth fields over a
//! coarse grid, sampled wherever something needs to know:
//!
//! - **Bathymetry**: depth below mean sea level at `res` metres per cell
//!   (negative depth is land, its height). Sampled bilinearly.
//! - **Tide**: two harmonics (lunar M2 and solar S2), so the range swings
//!   between springs and neaps over a fortnight. The tidal *stream* floods
//!   while the tide rises and ebbs while it falls, strongest at mid-tide and
//!   slack at high and low water.
//! - **Currents**: a steady drift and the tidal stream, each a far-field
//!   velocity bent round the coast by a depth-weighted potential flow (solved
//!   once). Water carried through a strait or over a shoal speeds up, because
//!   the same transport has to pass through less section; it slows in bays.
//! - **Waves**: one directional spectrum for the sea at large (10 frequencies
//!   by 16 directions). The wind feeds energy into the bins that run with it,
//!   up to a fetch-limited JONSWAP sea, taking longer for longer waves; bins
//!   it no longer feeds decay slowly into swell, which outlasts the wind by
//!   hours. The surface is a sum of the bins' sinusoids, scaled locally for
//!   shelter (fetch to the land upwind), shoaling and breaking in shallows.
//! - **Whirlpools**: eddies shed at headlands where the tidal stream runs
//!   hardest, which come and go with the tide, plus any the game places
//!   (a maelstrom that turns with every flood). Each is a vortex that adds to
//!   the current, draws things inward and dips the surface.
//!
//! Space is in metres: x east, y south (so north is up on screen, as on the
//! grid), z up from mean sea level. Time is seconds since the sea was made,
//! or whatever the game sets with [`Sea::set_time`]. Headings are compass
//! radians: 0 north, PI/2 east.

use crate::atlas::{Atlas, REGION};
use crate::color::Rgb;
use crate::frame::{Cell, Frame};
use crate::geom::Rect;
use crate::rng::{hash3, Rng};
use std::f32::consts::{PI, TAU};

/// Gravity, m/s^2.
pub const G: f32 = 9.81;
/// Sea water, kg/m^3.
pub const RHO_SEA: f32 = 1025.0;
/// Air, kg/m^3.
pub const RHO_AIR: f32 = 1.225;
/// Lunar semidiurnal period (M2), seconds.
pub const M2: f32 = 44_714.0;
/// Solar semidiurnal period (S2), seconds.
pub const S2: f32 = 43_200.0;

const NF: usize = 10;
const ND: usize = 16;
const F_LO: f32 = 0.05;
const F_HI: f32 = 0.5;
/// Most wave components summed per sample.
const MAX_COMPONENTS: usize = 48;
/// Longest side of the grid the currents and shelter are solved on.
const FIELD_MAX: i32 = 256;

#[derive(Clone, Debug)]
pub struct SeaSettings {
    /// Metres per bathymetry cell.
    pub res: f32,
    /// Range from low to high water at springs, metres.
    pub tide_range: f32,
    /// Solar to lunar amplitude (0.46 on Earth): how much smaller neaps are.
    pub spring_neap: f32,
    /// The flood stream far out at sea at springs, m/s (x east, y south).
    /// It reverses on the ebb; straits and headlands run faster.
    pub tidal_stream: (f32, f32),
    /// A steady current far out at sea, m/s.
    pub steady: (f32, f32),
    /// Open-sea fetch for the wind (metres): how far the wind has blown over
    /// water to build the sea that rolls in from beyond the map.
    pub open_fetch: f32,
    /// Tidal stream (m/s) at a headland above which eddies start to spin off.
    pub eddy_threshold: f32,
    pub seed: u32,
}

impl Default for SeaSettings {
    fn default() -> Self {
        SeaSettings {
            res: 16.0,
            tide_range: 1.0,
            spring_neap: 0.46,
            tidal_stream: (0.0, 0.0),
            steady: (0.0, 0.0),
            open_fetch: 150_000.0,
            eddy_threshold: 1.0,
            seed: 1,
        }
    }
}

/// A vortex in the sea.
#[derive(Clone, Copy, Debug)]
pub struct Whirlpool {
    pub x: f32,
    pub y: f32,
    /// Radius of the core, where the water turns fastest (m).
    pub radius: f32,
    /// Speed of the water at the core's rim (m/s) at full strength.
    pub strength: f32,
    /// +1 or -1: which way it turns (+1 is clockwise on screen).
    pub spin: f32,
    /// Strength follows the tidal stream (strongest at mid-tide, gone at
    /// slack water), as at a tidal race.
    pub tidal: bool,
    /// Seconds it has existed, and how long it lasts (infinite: forever).
    pub age: f32,
    pub life: f32,
    /// The headland it was shed from, if any.
    pub site: Option<usize>,
    /// Current strength factor, 0..1 (fades in and out, follows the tide).
    pub now: f32,
}

impl Whirlpool {
    /// A lasting whirlpool, e.g. a maelstrom: strongest when the tide runs.
    pub fn new(x: f32, y: f32, radius: f32, strength: f32, spin: f32) -> Whirlpool {
        Whirlpool { x, y, radius, strength, spin: spin.signum(), tidal: true, age: 0.0, life: f32::INFINITY, site: None, now: 0.0 }
    }

    /// Water velocity (m/s) it adds at an offset (dx, dy) from its centre,
    /// and how far it dips the surface there (m, negative).
    #[inline]
    pub fn effect(&self, dx: f32, dy: f32) -> (f32, f32, f32) {
        let r2 = dx * dx + dy * dy;
        let rc = self.radius.max(1.0);
        if r2 > 64.0 * rc * rc || self.now <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let r = r2.sqrt().max(1e-3);
        let q = r / rc;
        let v = self.strength * self.now;
        // Peaks at the core's rim, falls off as 1/r outside it.
        let vt = v * 2.0 * q / (1.0 + q * q);
        let (tx, ty) = (-dy / r * self.spin, dx / r * self.spin);
        let inward = 0.2 * vt;
        let dip = -(v * v / G) / (1.0 + q * q);
        (tx * vt - dx / r * inward, ty * vt - dy / r * inward, dip)
    }
}

/// Where a headland makes eddies when the stream runs hard past it.
#[derive(Clone, Copy, Debug)]
pub struct EddySite {
    pub x: f32,
    pub y: f32,
    /// Unit vector toward the nearest land.
    pub to_land: (f32, f32),
    /// Tidal stream here per unit of far-field stream.
    pub gain: f32,
}

/// The sea's state in round numbers, for reports and the weather glass.
#[derive(Clone, Copy, Debug, Default)]
pub struct SeaState {
    /// Significant wave height (m) of everything, the wind sea and the swell.
    pub hs: f32,
    pub wind_hs: f32,
    pub swell_hs: f32,
    /// Peak period (s) and the direction the waves run toward (radians, x east, y south).
    pub tp: f32,
    pub dir: f32,
    pub swell_dir: f32,
    pub swell_tp: f32,
}

#[derive(Clone, Copy, Debug)]
struct Component {
    a: f32,
    kx: f32,
    ky: f32,
    w: f32,
    phase: f32,
}

#[derive(Clone, Debug)]
pub struct Sea {
    pub settings: SeaSettings,
    /// Bathymetry size in cells.
    pub w: i32,
    pub h: i32,
    pub res: f32,
    /// Depth below mean sea level per cell (m); negative is land.
    pub depth: Vec<f32>,
    /// Seconds.
    pub time: f64,
    /// The wind (m/s, the way it blows) the waves last felt.
    pub wind: (f32, f32),
    pub whirlpools: Vec<Whirlpool>,
    pub sites: Vec<EddySite>,
    /// Variance (m^2) per frequency x direction bin.
    spectrum: Vec<f32>,
    bins: Vec<(f32, f32, f32, f32)>, // kx, ky, omega, phase
    comps: Vec<Component>,
    total_amp: f32,
    hs_total: f32,
    tp: f32,
    mean_dir: f32,
    // Field grid: currents and shelter.
    fw: i32,
    fh: i32,
    fres: f32,
    /// Current per unit far-field velocity along x and along y: (vx, vy) each.
    flow_x: Vec<(f32, f32)>,
    flow_y: Vec<(f32, f32)>,
    shelter: Vec<f32>,
    /// Field cells that are water.
    fwet: Vec<bool>,
    shelter_dir: f32,
    shelter_age: f32,
    rng: Rng,
    tide_phase: (f32, f32),
}

#[inline]
fn freq(i: usize) -> f32 {
    F_LO * (F_HI / F_LO).powf(i as f32 / (NF - 1) as f32)
}

#[inline]
fn dfreq(i: usize) -> f32 {
    let r = (F_HI / F_LO).powf(1.0 / (NF - 1) as f32);
    freq(i) * (r.sqrt() - 1.0 / r.sqrt())
}

#[inline]
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// JONSWAP spectral density (m^2/Hz) for wind `u` (m/s) over fetch `fetch` (m).
fn jonswap(f: f32, u: f32, fetch: f32) -> f32 {
    if u < 0.5 {
        return 0.0;
    }
    let x = (G * fetch / (u * u)).max(1.0);
    let fp = (3.5 * G / u * x.powf(-0.33)).max(0.13 * G / u);
    let alpha = (0.076 * x.powf(-0.22)).max(0.0081);
    let sigma = if f <= fp { 0.07 } else { 0.09 };
    let r = (-(f - fp).powi(2) / (2.0 * sigma * sigma * fp * fp)).exp();
    alpha * G * G * (TAU).powi(-4) * f.powi(-5) * (-1.25 * (fp / f).powi(4)).exp() * 3.3f32.powf(r)
}

/// Wavenumber times depth for angular frequency `w` in depth `h`
/// (Eckart's approximation to the dispersion relation, within ~5%).
#[inline]
fn kh_of(w: f32, h: f32) -> f32 {
    let k0h = w * w / G * h;
    k0h / k0h.tanh().sqrt().max(1e-4)
}

/// How much a wave of period `t` grows coming into depth `h` (Green's law
/// via group velocity), before it breaks.
#[inline]
fn shoaling(t: f32, h: f32) -> f32 {
    let w = TAU / t.max(1.0);
    let kh = kh_of(w, h.max(0.05));
    if kh > 3.0 {
        return 1.0;
    }
    let n = 0.5 * (1.0 + 2.0 * kh / (2.0 * kh).sinh().max(1e-4));
    let c_over_c0 = kh.tanh();
    (1.0 / (2.0 * n * c_over_c0)).sqrt().min(2.5)
}

impl Sea {
    /// A sea over `w` x `h` cells of `settings.res` metres, with the depth
    /// (m below mean sea level; negative is land) at each cell centre given
    /// by `depth(x, y)` in metres.
    pub fn new(w: i32, h: i32, settings: SeaSettings, depth: impl Fn(f32, f32) -> f32) -> Sea {
        assert!(w > 0 && h > 0);
        let res = settings.res;
        let mut d = vec![0.0; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                d[(y * w + x) as usize] = depth((x as f32 + 0.5) * res, (y as f32 + 0.5) * res);
            }
        }
        let mut rng = Rng::new(settings.seed as u64 ^ 0x5EA5);
        let mut bins = Vec::with_capacity(NF * ND);
        for i in 0..NF {
            for j in 0..ND {
                let om = TAU * freq(i);
                let k = om * om / G;
                let th = j as f32 * TAU / ND as f32 + (rng.f32() - 0.5) * 0.3;
                bins.push((k * th.cos(), k * th.sin(), om, rng.f32() * TAU));
            }
        }
        let cf = ((w.max(h) + FIELD_MAX - 1) / FIELD_MAX).max(1);
        let (fw, fh) = ((w + cf - 1) / cf, (h + cf - 1) / cf);
        let n = (fw * fh) as usize;
        let tide_phase = (rng.f32() * TAU, rng.f32() * TAU);
        let mut sea = Sea {
            settings,
            w,
            h,
            res,
            depth: d,
            time: 0.0,
            wind: (0.0, 0.0),
            whirlpools: Vec::new(),
            sites: Vec::new(),
            spectrum: vec![0.0; NF * ND],
            bins,
            comps: Vec::new(),
            total_amp: 0.0,
            hs_total: 0.0,
            tp: 8.0,
            mean_dir: 0.0,
            fw,
            fh,
            fres: res * cf as f32,
            flow_x: vec![(0.0, 0.0); n],
            flow_y: vec![(0.0, 0.0); n],
            shelter: vec![1.0; n],
            fwet: vec![true; n],
            shelter_dir: f32::NAN,
            shelter_age: 0.0,
            rng,
            tide_phase,
        };
        sea.solve_flow();
        sea.find_eddy_sites();
        sea
    }

    /// A sea over an authored atlas: depth is the negative of the atlas's
    /// elevation (Catmull-Rom smoothed), at `settings.res` metres a cell.
    pub fn from_atlas(atlas: &Atlas, settings: SeaSettings) -> Sea {
        let res = settings.res;
        let r = REGION as f32;
        let (w, h) = (((atlas.w as f32 * r) / res).ceil() as i32, ((atlas.h as f32 * r) / res).ceil() as i32);
        Sea::new(w, h, settings, |x, y| -atlas.elevation_at(x / r, y / r))
    }

    /// Width and height in metres.
    pub fn size(&self) -> (f32, f32) {
        (self.w as f32 * self.res, self.h as f32 * self.res)
    }

    pub fn set_time(&mut self, secs: f64) {
        self.time = secs;
    }

    // ---------------------------------------------------------------- depth

    /// Depth of the bed below mean sea level (m) at a point; negative is land.
    pub fn bed(&self, x: f32, y: f32) -> f32 {
        let (gx, gy) = (x / self.res - 0.5, y / self.res - 0.5);
        let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
        let (fx, fy) = (gx - ix as f32, gy - iy as f32);
        let at = |x: i32, y: i32| self.depth[(y.clamp(0, self.h - 1) * self.w + x.clamp(0, self.w - 1)) as usize];
        let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * fx;
        let bot = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * fx;
        top + (bot - top) * fy
    }

    /// Height of the ground above mean sea level (negative under the sea).
    pub fn ground(&self, x: f32, y: f32) -> f32 {
        -self.bed(x, y)
    }

    /// Depth of water right now (bed plus tide), ignoring waves.
    pub fn water_depth(&self, x: f32, y: f32) -> f32 {
        self.bed(x, y) + self.tide()
    }

    // ----------------------------------------------------------------- tide

    fn tide_parts(&self, t: f64) -> (f32, f32, f32, f32) {
        let a1 = (TAU as f64 * t / M2 as f64) as f32 + self.tide_phase.0;
        let a2 = (TAU as f64 * t / S2 as f64) as f32 + self.tide_phase.1;
        (a1.cos(), a2.cos(), a1.sin(), a2.sin())
    }

    /// Height of the tide (m above mean sea level) at time `t`.
    pub fn tide_at(&self, t: f64) -> f32 {
        let s = &self.settings;
        let m2 = s.tide_range * 0.5 / (1.0 + s.spring_neap);
        let (c1, c2, _, _) = self.tide_parts(t);
        m2 * c1 + m2 * s.spring_neap * c2
    }

    pub fn tide(&self) -> f32 {
        self.tide_at(self.time)
    }

    /// The tidal stream as a fraction of its spring strength: positive on
    /// the flood (tide rising), negative on the ebb, 0 at slack water.
    pub fn stream_at(&self, t: f64) -> f32 {
        let sn = self.settings.spring_neap;
        let (_, _, s1, s2) = self.tide_parts(t);
        -(s1 + sn * s2) / (1.0 + sn)
    }

    pub fn stream(&self) -> f32 {
        self.stream_at(self.time)
    }

    /// Whether the tide is rising, and how far through springs (1) to neaps (0) we are.
    pub fn tide_phase(&self) -> (bool, f32) {
        let sn = self.settings.spring_neap;
        let beat = ((TAU as f64 * self.time * (1.0 / S2 as f64 - 1.0 / M2 as f64)) as f32 + self.tide_phase.1 - self.tide_phase.0).cos();
        // The two harmonics' combined amplitude, from 1 - sn (neaps) to 1 + sn (springs).
        let env = (1.0 + sn * sn + 2.0 * sn * beat).max(0.0).sqrt();
        (self.stream() > 0.0, ((env - (1.0 - sn)) / (2.0 * sn).max(1e-6)).clamp(0.0, 1.0))
    }

    // ------------------------------------------------------------- currents

    /// Solve the flow round the coast for a unit current entering across the
    /// west edge (and, separately, the north edge) and leaving across the
    /// opposite one. Any far-field current is a sum of the two.
    fn solve_flow(&mut self) {
        let (fw, fh) = (self.fw, self.fh);
        let cf = (self.fres / self.res).round() as i32;
        let n = (fw * fh) as usize;
        // Mean depth over each field cell.
        let mut h = vec![0.0f32; n];
        for fy in 0..fh {
            for fx in 0..fw {
                let (mut sum, mut cnt) = (0.0, 0);
                for y in fy * cf..((fy + 1) * cf).min(self.h) {
                    for x in fx * cf..((fx + 1) * cf).min(self.w) {
                        sum += self.depth[(y * self.w + x) as usize];
                        cnt += 1;
                    }
                }
                h[(fy * fw + fx) as usize] = if cnt > 0 { sum / cnt as f32 } else { 0.0 };
            }
        }
        self.fwet = h.iter().map(|&d| d > 0.5).collect();
        let fres = self.fres;
        for axis in 0..2 {
            let (phi, open) = solve_potential(&h, fw, fh, fres, axis);
            let out = if axis == 0 { &mut self.flow_x } else { &mut self.flow_y };
            for y in 0..fh {
                for x in 0..fw {
                    let i = (y * fw + x) as usize;
                    if !open[i] {
                        out[i] = (0.0, 0.0);
                        continue;
                    }
                    let grad = |a: Option<usize>, b: Option<usize>| -> f32 {
                        match (a.filter(|&j| open[j]), b.filter(|&j| open[j])) {
                            (Some(a), Some(b)) => (phi[b] - phi[a]) / (2.0 * fres),
                            (Some(a), None) => (phi[i] - phi[a]) / fres,
                            (None, Some(b)) => (phi[b] - phi[i]) / fres,
                            (None, None) => 0.0,
                        }
                    };
                    let w_ = (x > 0).then(|| i - 1);
                    let e_ = (x < fw - 1).then(|| i + 1);
                    let n_ = (y > 0).then(|| i - fw as usize);
                    let s_ = (y < fh - 1).then(|| i + fw as usize);
                    out[i] = (grad(w_, e_), grad(n_, s_));
                }
            }
        }
    }

    /// Bilinear sample of a field-grid vector field.
    fn field_vec(&self, f: &[(f32, f32)], x: f32, y: f32) -> (f32, f32) {
        let (gx, gy) = (x / self.fres - 0.5, y / self.fres - 0.5);
        let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
        let (tx, ty) = (gx - ix as f32, gy - iy as f32);
        let at = |x: i32, y: i32| f[(y.clamp(0, self.fh - 1) * self.fw + x.clamp(0, self.fw - 1)) as usize];
        let (a, b, c, d) = (at(ix, iy), at(ix + 1, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
        let l = |p: f32, q: f32, t: f32| p + (q - p) * t;
        (l(l(a.0, b.0, tx), l(c.0, d.0, tx), ty), l(l(a.1, b.1, tx), l(c.1, d.1, tx), ty))
    }

    fn field_scalar(&self, f: &[f32], x: f32, y: f32) -> f32 {
        let (gx, gy) = (x / self.fres - 0.5, y / self.fres - 0.5);
        let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
        let (tx, ty) = (gx - ix as f32, gy - iy as f32);
        let at = |x: i32, y: i32| f[(y.clamp(0, self.fh - 1) * self.fw + x.clamp(0, self.fw - 1)) as usize];
        let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * tx;
        let bot = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * tx;
        top + (bot - top) * ty
    }

    /// The steady current plus the tidal stream (no eddies) at a point, m/s,
    /// for a given stream fraction.
    fn base_current(&self, x: f32, y: f32, stream: f32) -> (f32, f32) {
        let s = &self.settings;
        let (ux, uy) = (s.steady.0 + stream * s.tidal_stream.0, s.steady.1 + stream * s.tidal_stream.1);
        if ux == 0.0 && uy == 0.0 {
            return (0.0, 0.0);
        }
        let fx = self.field_vec(&self.flow_x, x, y);
        let fy = self.field_vec(&self.flow_y, x, y);
        (ux * fx.0 + uy * fy.0, ux * fx.1 + uy * fy.1)
    }

    /// The water's velocity at a point (m/s): steady current, tidal stream
    /// and whirlpools. Zero on land.
    pub fn current(&self, x: f32, y: f32) -> (f32, f32) {
        if self.bed(x, y) <= 0.0 {
            return (0.0, 0.0);
        }
        let (mut vx, mut vy) = self.base_current(x, y, self.stream());
        for wp in &self.whirlpools {
            let (ex, ey, _) = wp.effect(x - wp.x, y - wp.y);
            vx += ex;
            vy += ey;
        }
        (vx, vy)
    }

    /// Headlands where the tidal stream runs faster than in the open sea and
    /// sheds eddies. Found once, from the tidal flow field.
    fn find_eddy_sites(&mut self) {
        self.sites.clear();
        let (tx, ty) = self.settings.tidal_stream;
        let mag = (tx * tx + ty * ty).sqrt();
        if mag < 1e-4 {
            return;
        }
        let (fw, fh) = (self.fw, self.fh);
        let gain = |i: usize| {
            let (a, b) = (self.flow_x[i], self.flow_y[i]);
            let (vx, vy) = ((tx * a.0 + ty * b.0) / mag, (tx * a.1 + ty * b.1) / mag);
            (vx * vx + vy * vy).sqrt()
        };
        let land = |x: i32, y: i32| {
            x >= 0 && y >= 0 && x < fw && y < fh && {
                !self.fwet[(y * fw + x) as usize]
            }
        };
        let mut cand: Vec<(f32, i32, i32, (f32, f32))> = Vec::new();
        for y in 1..fh - 1 {
            for x in 1..fw - 1 {
                if land(x, y) {
                    continue;
                }
                let (mut lx, mut ly, mut nl) = (0.0, 0.0, 0);
                for (dx, dy) in crate::geom::DIRS8 {
                    if land(x + dx, y + dy) {
                        lx += dx as f32;
                        ly += dy as f32;
                        nl += 1;
                    }
                }
                // A headland: land on one side only, not a bay or a channel end.
                if nl == 0 || nl > 4 {
                    continue;
                }
                let g = gain((y * fw + x) as usize);
                if g < 1.3 {
                    continue;
                }
                let l = (lx * lx + ly * ly).sqrt();
                if l < 0.5 {
                    continue;
                }
                cand.push((g, x, y, (lx / l, ly / l)));
            }
        }
        cand.sort_by(|a, b| b.0.total_cmp(&a.0));
        let sep = (400.0 / self.fres).max(4.0);
        for (g, x, y, tl) in cand {
            let (px, py) = ((x as f32 + 0.5) * self.fres, (y as f32 + 0.5) * self.fres);
            if self.sites.iter().any(|s| ((s.x - px).powi(2) + (s.y - py).powi(2)).sqrt() < sep * self.fres) {
                continue;
            }
            self.sites.push(EddySite { x: px, y: py, to_land: tl, gain: g });
            if self.sites.len() >= 32 {
                break;
            }
        }
    }

    // ---------------------------------------------------------------- waves

    /// Raise the sea straight to what this wind would build given time (for
    /// starting a scene mid-voyage rather than in a flat calm).
    pub fn settle(&mut self, wind: (f32, f32)) {
        self.wind = wind;
        let target = self.target_spectrum(wind);
        self.spectrum.copy_from_slice(&target);
        self.refresh_components();
        self.shelter_age = f32::INFINITY;
        self.update_shelter(0.0);
    }

    fn target_spectrum(&self, wind: (f32, f32)) -> Vec<f32> {
        let u = (wind.0 * wind.0 + wind.1 * wind.1).sqrt();
        let th = wind.1.atan2(wind.0);
        let mut weights = [0.0f32; ND];
        for (j, wgt) in weights.iter_mut().enumerate() {
            let d = wrap(j as f32 * TAU / ND as f32 - th);
            if d.abs() < PI / 2.0 {
                *wgt = d.cos().powi(2);
            }
        }
        let norm: f32 = weights.iter().sum::<f32>().max(1e-6);
        let mut out = vec![0.0; NF * ND];
        for i in 0..NF {
            let v = jonswap(freq(i), u, self.settings.open_fetch) * dfreq(i);
            for j in 0..ND {
                out[i * ND + j] = v * weights[j] / norm;
            }
        }
        out
    }

    /// Advance the sea `dt` seconds under `wind` (m/s, the way it blows):
    /// waves grow or decay toward the wind, eddies come and go, and the
    /// clock moves on.
    pub fn step(&mut self, dt: f32, wind: (f32, f32)) {
        self.time += dt as f64;
        self.wind = wind;
        let target = self.target_spectrum(wind);
        let th = wind.1.atan2(wind.0);
        for i in 0..NF {
            let f = freq(i);
            let period = 1.0 / f;
            // A sea takes about an hour to build 8 s waves, minutes for chop.
            let grow = 450.0 * period;
            // Swell outlasts the wind: a day for long waves, minutes for chop.
            let decay = 3000.0 * (0.3 / f).powi(3);
            for j in 0..ND {
                let k = i * ND + j;
                let (e, t) = (self.spectrum[k], target[k]);
                let with_wind = wrap(j as f32 * TAU / ND as f32 - th).abs() < PI / 2.0;
                let tau = if t > e { grow } else if with_wind && t > 0.0 { grow * 2.0 } else { decay };
                self.spectrum[k] = e + (t - e) * (dt / tau).min(1.0);
            }
        }
        self.refresh_components();
        self.update_shelter(dt);
        self.step_whirlpools(dt);
    }

    fn refresh_components(&mut self) {
        self.comps.clear();
        let (mut m0, mut best, mut sx, mut sy) = (0.0f32, (0.0f32, 0usize), 0.0f32, 0.0f32);
        for (k, &v) in self.spectrum.iter().enumerate() {
            m0 += v;
            let (i, j) = (k / ND, k % ND);
            let fsum: f32 = v;
            let th = j as f32 * TAU / ND as f32;
            sx += v * th.cos();
            sy += v * th.sin();
            let band: f32 = (0..ND).map(|jj| self.spectrum[i * ND + jj]).sum();
            if band > best.0 {
                best = (band, i);
            }
            let a = (2.0 * fsum).sqrt();
            if a > 0.004 {
                let (kx, ky, w, ph) = self.bins[k];
                self.comps.push(Component { a, kx, ky, w, phase: ph });
            }
        }
        self.comps.sort_by(|a, b| b.a.total_cmp(&a.a));
        self.comps.truncate(MAX_COMPONENTS);
        self.total_amp = self.comps.iter().map(|c| c.a).sum();
        self.hs_total = 4.0 * m0.sqrt();
        self.tp = 1.0 / freq(best.1);
        if sx.abs() + sy.abs() > 0.0 {
            self.mean_dir = sy.atan2(sx);
        }
    }

    /// Shelter from land upwind: the fetch the waves have had to grow over,
    /// up to 5 km, toward the mean wave direction. Recomputed when the waves
    /// swing round or every few minutes.
    fn update_shelter(&mut self, dt: f32) {
        self.shelter_age += dt;
        let turned = !self.shelter_dir.is_finite() || wrap(self.mean_dir - self.shelter_dir).abs() > 0.2;
        if !turned && self.shelter_age < 600.0 {
            return;
        }
        self.shelter_dir = self.mean_dir;
        self.shelter_age = 0.0;
        let reference = 5000.0f32;
        let (cx, cy) = (-self.mean_dir.cos(), -self.mean_dir.sin());
        let step = self.fres;
        let steps = (reference / step).ceil() as i32;
        for fy in 0..self.fh {
            for fx in 0..self.fw {
                let (x0, y0) = ((fx as f32 + 0.5) * self.fres, (fy as f32 + 0.5) * self.fres);
                let mut fetch = reference;
                for s in 1..=steps {
                    let (x, y) = (x0 + cx * step * s as f32, y0 + cy * step * s as f32);
                    let (sw, sh) = self.size();
                    if x < 0.0 || y < 0.0 || x >= sw || y >= sh {
                        break; // the open sea beyond the map
                    }
                    if self.bed(x, y) <= 0.0 {
                        fetch = step * s as f32;
                        break;
                    }
                }
                self.shelter[(fy * self.fw + fx) as usize] = (fetch / reference).sqrt().max(0.06);
            }
        }
    }

    /// How tall the waves are here relative to the open sea (shelter,
    /// shoaling, breaking); 0 on land.
    pub fn wave_scale(&self, x: f32, y: f32) -> f32 {
        let h = self.water_depth(x, y);
        if h <= 0.05 {
            return 0.0;
        }
        let s = self.field_scalar(&self.shelter, x, y) * shoaling(self.tp, h);
        if self.hs_total > 0.0 {
            s.min(0.55 * h / self.hs_total)
        } else {
            s
        }
    }

    /// Waves breaking here (surf on a shoal or a beach).
    pub fn breaking(&self, x: f32, y: f32) -> bool {
        let h = self.water_depth(x, y);
        h > 0.05 && self.hs_total * self.field_scalar(&self.shelter, x, y) * shoaling(self.tp, h) > 0.5 * h
    }

    /// Height of the waves alone at a point and time, before local scaling.
    #[inline]
    fn raw_waves(&self, x: f32, y: f32, t: f32) -> f32 {
        let mut z = 0.0;
        for c in &self.comps {
            z += c.a * (c.kx * x + c.ky * y - c.w * t + c.phase).cos();
        }
        z
    }

    fn wave_time(&self) -> f32 {
        // Waves repeat; keep f32 time small so phases stay precise.
        (self.time % 86_400.0) as f32
    }

    /// The sea surface (m above mean sea level) at a point now: tide, waves
    /// and whirlpool funnels.
    pub fn height(&self, x: f32, y: f32) -> f32 {
        self.height_in(x, y, 0.0)
    }

    /// The surface `ahead` seconds from now (waves only move on; the tide
    /// and eddies are taken as they are). For sub-stepping between sea steps.
    pub fn height_in(&self, x: f32, y: f32, ahead: f32) -> f32 {
        let mut z = self.tide();
        let s = self.wave_scale(x, y);
        if s > 0.0 {
            z += s * self.raw_waves(x, y, self.wave_time() + ahead);
        }
        for wp in &self.whirlpools {
            z += wp.effect(x - wp.x, y - wp.y).2;
        }
        z
    }

    /// An upper bound on how high the sea can stand anywhere right now.
    pub fn max_height(&self) -> f32 {
        self.tide() + 2.5 * self.total_amp.min(2.0 * self.hs_total.max(0.01))
    }

    /// The sea state in round numbers.
    pub fn state(&self) -> SeaState {
        let th = self.wind.1.atan2(self.wind.0);
        let windy = (self.wind.0.powi(2) + self.wind.1.powi(2)).sqrt() > 0.5;
        let (mut wv, mut sv, mut sx, mut sy, mut best) = (0.0, 0.0, 0.0, 0.0, (0.0f32, 0usize));
        for (k, &v) in self.spectrum.iter().enumerate() {
            let (i, j) = (k / ND, k % ND);
            let d = j as f32 * TAU / ND as f32;
            if windy && wrap(d - th).abs() < PI / 2.0 {
                wv += v;
            } else {
                sv += v;
                sx += v * d.cos();
                sy += v * d.sin();
                if v > best.0 {
                    best = (v, i);
                }
            }
        }
        SeaState {
            hs: self.hs_total,
            wind_hs: 4.0 * f32::sqrt(wv),
            swell_hs: 4.0 * f32::sqrt(sv),
            tp: self.tp,
            dir: self.mean_dir,
            swell_dir: sy.atan2(sx),
            swell_tp: 1.0 / freq(best.1),
        }
    }

    /// Wavelength of the peak waves (m).
    pub fn peak_wavelength(&self) -> f32 {
        G * self.tp * self.tp / TAU
    }

    // ----------------------------------------------------------- whirlpools

    pub fn add_whirlpool(&mut self, w: Whirlpool) {
        self.whirlpools.push(w);
    }

    fn step_whirlpools(&mut self, dt: f32) {
        let stream = self.stream();
        let thr = self.settings.eddy_threshold;
        // Eddies shed at headlands when the stream runs hard past them.
        for (si, site) in self.sites.clone().into_iter().enumerate() {
            let (cx, cy) = self.base_current(site.x, site.y, stream);
            let speed = (cx * cx + cy * cy).sqrt();
            if speed < thr || self.whirlpools.iter().any(|w| w.site == Some(si)) {
                continue;
            }
            // Roughly one a quarter-hour while the race runs hard.
            if !self.rng.chance((dt / 900.0) * (speed / thr - 0.5)) {
                continue;
            }
            let (ux, uy) = (cx / speed, cy / speed);
            let rc = (12.0 + 10.0 * speed).min(60.0);
            // Shed into the lee: downstream of the point, a little off the land.
            let (x, y) = (site.x + ux * 3.0 * rc - site.to_land.0 * rc, site.y + uy * 3.0 * rc - site.to_land.1 * rc);
            if self.bed(x, y) < 2.0 {
                continue;
            }
            // Turning so that the water next to the land runs back upstream.
            let perp = (-site.to_land.1, site.to_land.0);
            let spin = -(perp.0 * cx + perp.1 * cy).signum();
            let life = 300.0 + self.rng.f32() * 900.0;
            self.whirlpools.push(Whirlpool {
                x,
                y,
                radius: rc,
                strength: 0.8 * speed,
                spin,
                tidal: true,
                age: 0.0,
                life,
                site: Some(si),
                now: 0.0,
            });
        }
        let mut moved = Vec::with_capacity(self.whirlpools.len());
        for wp in &self.whirlpools {
            let (cx, cy) = self.base_current(wp.x, wp.y, stream);
            moved.push((cx, cy));
        }
        for (wp, (cx, cy)) in self.whirlpools.iter_mut().zip(moved) {
            wp.age += dt;
            let fade = if wp.life.is_finite() {
                (wp.age / 60.0).min(1.0).min(((wp.life - wp.age) / 120.0).max(0.0))
            } else {
                1.0
            };
            let tide = if wp.tidal { stream.abs() } else { 1.0 };
            wp.now = fade * tide;
            if wp.site.is_some() {
                // Shed eddies drift off with the stream.
                wp.x += cx * 0.5 * dt;
                wp.y += cy * 0.5 * dt;
            }
        }
        self.whirlpools.retain(|w| w.age < w.life);
    }

    // ------------------------------------------------------------ rendering

    /// What the sea (or the shore) looks like at a point, for a picture at
    /// `mpc` metres per cell. Close in, each crest is drawn; far out, the
    /// waves become a texture whose density follows the sea state.
    pub fn look(&self, x: f32, y: f32, mpc: f32) -> Cell {
        let bed = self.bed(x, y);
        let tide = self.tide();
        let land = -bed - tide;
        if land > 0.0 {
            return land_cell(land, x, y);
        }
        let depth = -land;
        let deep = Rgb(12, 34, 70);
        let shoal = Rgb(28, 92, 104);
        let bg0 = shoal.lerp(deep, (depth / 25.0).sqrt());
        let s = self.wave_scale(x, y);
        let hs = (self.hs_total * s).max(0.02);
        let t = self.wave_time();
        let hx = ((x / mpc.max(0.25)).floor() as i32, (y / mpc.max(0.25)).floor() as i32);
        let hash = hash3(hx.0, hx.1, 0, self.settings.seed);

        // Whirlpools: draw the turning water.
        for wp in &self.whirlpools {
            let (dx, dy) = (x - wp.x, y - wp.y);
            let r = (dx * dx + dy * dy).sqrt();
            if wp.now > 0.2 && r < 2.2 * wp.radius {
                if r < 0.35 * wp.radius {
                    return Cell::new('@', Rgb(140, 170, 190), Rgb(6, 18, 40));
                }
                // Glyph along the direction the water turns.
                let a = (dx * wp.spin).atan2(-dy * wp.spin) + PI / 2.0;
                let k = ((a / (PI / 4.0)).round() as i32).rem_euclid(4);
                let ch = ['-', '\\', '|', '/'][k as usize];
                let foam = (1.0 - r / (2.2 * wp.radius)) * wp.now;
                return Cell::new(ch, Rgb(120, 160, 190).lerp(Rgb(230, 240, 245), foam), bg0.scale(0.8));
            }
        }

        let wind = (self.wind.0.powi(2) + self.wind.1.powi(2)).sqrt();
        let surf = self.breaking(x, y);
        let detail = (self.peak_wavelength() / (3.0 * mpc.max(0.25))).clamp(0.0, 1.0);
        if detail > 0.3 {
            let n = (s * self.raw_waves(x, y, t)) / (0.5 * hs);
            let bg = if n > 0.0 { bg0.lerp(Rgb(60, 110, 150), n * 0.35) } else { bg0.scale(1.0 + 0.12 * n.max(-2.0)) };
            let whitecap = wind > 8.0 && n > 1.1 && (hash & 7) < ((wind - 7.0) as u32).min(7);
            if surf && n > 0.3 {
                return Cell::new(if hash & 1 == 0 { '~' } else { '*' }, Rgb(235, 240, 240), bg.lerp(Rgb(150, 180, 190), 0.4));
            }
            if whitecap {
                return Cell::new('*', Rgb(230, 236, 240), bg);
            }
            let (ch, fg) = if n > 0.9 {
                ('~', Rgb(170, 200, 225))
            } else if n > 0.35 {
                ('~', Rgb(90, 130, 175))
            } else if n < -0.9 {
                (' ', bg)
            } else {
                ('-', bg.lerp(Rgb(90, 120, 160), 0.35))
            };
            Cell::new(ch, fg, bg)
        } else {
            // A texture: more marks the higher the sea.
            let p = (hs / 3.0).clamp(0.05, 0.6);
            let r = (hash & 0xFFFF) as f32 / 65535.0;
            if surf {
                return Cell::new('~', Rgb(220, 230, 230), bg0.lerp(Rgb(150, 180, 190), 0.3));
            }
            if r < p {
                let cap = wind > 8.0 && r < p * 0.25;
                Cell::new(if cap { '*' } else { '~' }, if cap { Rgb(220, 230, 236) } else { Rgb(80, 120, 165) }, bg0)
            } else {
                Cell::new(' ', bg0, bg0)
            }
        }
    }

    /// Draw a chart of the sea into `area`: centred on (cx, cy) metres at
    /// `mpc` metres per cell, rotated so that compass heading `up` points up
    /// the screen (0 for north-up). `wide` draws two characters per cell.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, frame: &mut Frame, area: Rect, cx: f32, cy: f32, mpc: f32, up: f32, wide: bool) {
        let cols = if wide { area.w / 2 } else { area.w };
        let (right, down) = screen_axes(up);
        for sy in 0..area.h {
            for sx in 0..cols {
                let (dx, dy) = ((sx - cols / 2) as f32 * mpc, (sy - area.h / 2) as f32 * mpc);
                let (x, y) = (cx + right.0 * dx + down.0 * dy, cy + right.1 * dx + down.1 * dy);
                let c = self.look(x, y, mpc);
                let px = area.x + if wide { sx * 2 } else { sx };
                frame.set(px, area.y + sy, c);
                if wide {
                    let c2 = if c.ch == '~' || c.ch == '-' { c } else { Cell::new(' ', c.bg, c.bg) };
                    frame.set(px + 1, area.y + sy, c2);
                }
            }
        }
    }
}

/// Depth-weighted velocity potential (m per m/s of far-field current) for a
/// unit current entering across the upstream edge (west for axis 0, north
/// for axis 1) and leaving across the downstream edge, with no flow through
/// the coast or the side edges. Conserving transport is what makes a strait
/// run fast: everything that comes in must get past the land somehow.
///
/// Each cell balances the flux through its faces, `sum t (phi_j - phi_i)`
/// with `t` the mean depth across the face, against the inflow; solved by
/// multigrid V-cycles (red-black Gauss-Seidel smoothing, the residual summed
/// onto a grid half the size, the correction carried back). Returns the
/// potential and which cells carry the flow (water connected to the
/// downstream edge).
fn solve_potential(h: &[f32], w: i32, ht: i32, cell: f32, axis: usize) -> (Vec<f32>, Vec<bool>) {
    let top = Level::build(h.iter().map(|&d| d as f64).collect(), w, ht, cell as f64, axis);
    let n = (w * ht) as usize;
    let along = |i: usize| if axis == 0 { (i % w as usize) as f64 } else { (i / w as usize) as f64 };
    let mut phi: Vec<f64> = (0..n).map(|i| along(i) * cell as f64).collect();
    let rhs = top.inflow.clone();
    let scale = top.inflow.iter().fold(0.0f64, |a, &b| a.max(b.abs())).max(1e-3);

    for _ in 0..60 {

        top.vcycle(&mut phi, &rhs);
        let r = top.residual(&phi, &rhs);
        if r.iter().fold(0.0f64, |a, &b| a.max(b.abs())) < 1e-4 * scale {
            break;
        }
    }
    (phi.iter().map(|&p| p as f32).collect(), top.open)
}

struct Level {
    w: i32,
    h: i32,
    depth: Vec<f64>,
    /// Water connected to the outflow edge.
    open: Vec<bool>,
    /// On the outflow edge: held fixed.
    fixed: Vec<bool>,
    /// Inflow flux into each cell on the inflow edge.
    inflow: Vec<f64>,
    coarse: Option<Box<Level>>,
}

impl Level {
    fn build(depth: Vec<f64>, w: i32, ht: i32, cell: f64, axis: usize) -> Level {
        let n = (w * ht) as usize;
        let wet: Vec<bool> = depth.iter().map(|&d| d > 0.5).collect();
        let along = |x: i32, y: i32| if axis == 0 { x } else { y };
        let last = if axis == 0 { w - 1 } else { ht - 1 };
        let mut open = vec![false; n];
        let mut fixed = vec![false; n];
        let mut stack = Vec::new();
        for y in 0..ht {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if along(x, y) == last && wet[i] {
                    open[i] = true;
                    fixed[i] = true;
                    stack.push(i);
                }
            }
        }
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w as usize) as i32, (i / w as usize) as i32);
            for (dx, dy) in crate::geom::DIRS4 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= ht {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if wet[j] && !open[j] {
                    open[j] = true;
                    stack.push(j);
                }
            }
        }
        let inflow: Vec<f64> = (0..n)
            .map(|i| {
                let (x, y) = ((i % w as usize) as i32, (i / w as usize) as i32);
                if open[i] && !fixed[i] && along(x, y) == 0 { depth[i] * cell } else { 0.0 }
            })
            .collect();
        let coarse = (w > 6 && ht > 6).then(|| {
            let (cw, ch) = ((w + 1) / 2, (ht + 1) / 2);
            let mut dc = vec![0.0f64; (cw * ch) as usize];
            for cy in 0..ch {
                for cx in 0..cw {
                    let (mut s, mut k) = (0.0, 0);
                    for y in cy * 2..(cy * 2 + 2).min(ht) {
                        for x in cx * 2..(cx * 2 + 2).min(w) {
                            let i = (y * w + x) as usize;
                            if open[i] {
                                s += depth[i];
                                k += 1;
                            }
                        }
                    }
                    dc[(cy * cw + cx) as usize] = if k > 0 { s / k as f64 } else { 0.0 };
                }
            }
            Box::new(Level::build(dc, cw, ch, cell * 2.0, axis))
        });
        Level { w, h: ht, depth, open, fixed, inflow, coarse }
    }

    /// Sum of t (phi_j - phi_i) over open neighbours, and the sum of t.
    #[inline]
    fn flux(&self, phi: &[f64], i: usize) -> (f64, f64) {
        let (w, ht) = (self.w, self.h);
        let (x, y) = ((i % w as usize) as i32, (i / w as usize) as i32);
        let (mut num, mut den) = (0.0, 0.0);
        for (dx, dy) in crate::geom::DIRS4 {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= ht {
                continue;
            }
            let j = (ny * w + nx) as usize;
            if self.open[j] {
                let t = 0.5 * (self.depth[i] + self.depth[j]);
                num += t * phi[j];
                den += t;
            }
        }
        (num, den)
    }

    fn smooth(&self, phi: &mut [f64], rhs: &[f64], sweeps: usize) {
        for _ in 0..sweeps {
            for colour in 0..2 {
                for y in 0..self.h {
                    for x in 0..self.w {
                        if (x + y) % 2 != colour {
                            continue;
                        }
                        let i = (y * self.w + x) as usize;
                        if !self.open[i] || self.fixed[i] {
                            continue;
                        }
                        let (num, den) = self.flux(phi, i);
                        if den > 0.0 {
                            phi[i] = (num - rhs[i]) / den;
                        }
                    }
                }
            }
        }
    }

    fn residual(&self, phi: &[f64], rhs: &[f64]) -> Vec<f64> {
        (0..phi.len())
            .map(|i| {
                if !self.open[i] || self.fixed[i] {
                    return 0.0;
                }
                let (num, den) = self.flux(phi, i);
                rhs[i] - (num - den * phi[i])
            })
            .collect()
    }

    fn vcycle(&self, phi: &mut [f64], rhs: &[f64]) {
        let Some(c) = &self.coarse else {
            self.smooth(phi, rhs, 60);
            return;
        };
        self.smooth(phi, rhs, 3);
        let r = self.residual(phi, rhs);
        let mut rc = vec![0.0f64; (c.w * c.h) as usize];
        for (i, &v) in r.iter().enumerate() {
            let (x, y) = ((i % self.w as usize) as i32, (i / self.w as usize) as i32);
            rc[((y / 2) * c.w + x / 2) as usize] += v;
        }
        let mut e = vec![0.0f64; rc.len()];
        c.vcycle(&mut e, &rc);
        for i in 0..phi.len() {
            if self.open[i] && !self.fixed[i] {
                let (x, y) = ((i % self.w as usize) as i32, (i / self.w as usize) as i32);
                phi[i] += e[((y / 2) * c.w + x / 2) as usize];
            }
        }
        self.smooth(phi, rhs, 3);
    }
}

/// The world directions of screen-right and screen-down when compass
/// heading `up` points up the screen.
pub fn screen_axes(up: f32) -> ((f32, f32), (f32, f32)) {
    let (s, c) = up.sin_cos();
    ((c, s), (-s, c))
}

fn land_cell(height: f32, x: f32, y: f32) -> Cell {
    let h = hash3(x as i32, y as i32, 1, 77);
    if height < 0.6 {
        Cell::new(if h & 1 == 0 { '.' } else { ',' }, Rgb(170, 150, 110), Rgb(92, 80, 56))
    } else if height < 3.0 {
        Cell::new('.', Rgb(206, 186, 130), Rgb(120, 104, 70))
    } else if height < 40.0 {
        Cell::new(['"', ',', '\'', '.'][(h & 3) as usize], Rgb(96, 150, 66), Rgb(34, 56, 26))
    } else {
        Cell::new('^', Rgb(150, 146, 140), Rgb(60, 58, 56))
    }
}
