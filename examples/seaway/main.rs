//! Seaway: two brigs on a modelled sea. The engine's sea, ships, round shot
//! and long sight together, for trying them out (not a game).
//!
//!     cargo run --release --example seaway                      (you con Dauntless)
//!     cargo run --release --example seaway -- --auto            (both captains fight it out)
//!     cargo run --release --example seaway -- --snapshot sea.html [--chart] [--battle] [--ticks 600] [--mpc 8] [--fire]
//!     cargo run --release --example seaway -- --replay action.html [--ticks 1800] [--every 6]
//!
//! An island and a strait with a tidal race, a maelstrom off the point,
//! two-metre tides, a freshening breeze that builds a sea, and a second brig.
//! The ship view is her own grid, bow up; the sea around her is sampled from
//! the fields. Tab cycles ship / chart / battle (both ships and the chart).
//! `--replay` records the action into a self-contained HTML player.

use std::f32::consts::{PI, TAU};
use std::fmt::Write as _;
use std::io;
use tachyon::ballistics::{elevation_for, Ballistics, Body, Placement, Shot, ShotEvent, V3};
use tachyon::craft::{self, Craft, CraftEvent, Floating, Foil, HullSpec, Medium};
use tachyon::prelude::*;
use tachyon::sea::{screen_axes, Sea, SeaSettings, Whirlpool};
use tachyon::sight::{self, Air, Murk, Target};

/// Game seconds per tick.
const SECS: f64 = 0.5;
const KEEL: i32 = 1;
const DECK: i32 = 5;
const GUN_SPEED: f32 = 440.0;
/// An 18-pound ball.
const BALL: f32 = 8.2;
/// Seconds for a brig's crews to sponge, load and run out.
const RELOAD: f32 = 75.0;

fn footprint(x: i32, y: i32) -> bool {
    if !(4..40).contains(&y) {
        return false;
    }
    let half = match y {
        4 => 1,
        5 => 2,
        6 => 3,
        7 => 4,
        _ => 5,
    };
    (9 - half..9 + half).contains(&x)
}

/// A brig drawn in her own grid: bow at the top, a hold on levels 2-4, her
/// deck on level 5 with guns along each side, two masts, a wheel aft.
fn brig(name: &str, x: f32, y: f32, heading: f32, seed: u64) -> Craft {
    let mats = Materials::standard();
    let (wood, water) = (mats.id("wood"), mats.id("water"));
    let mut world = World::new(18, 44, 10, mats);
    let mut area = 0.0f32;
    for gy in 0..44 {
        for gx in 0..18 {
            if !footprint(gx, gy) {
                continue;
            }
            area += 1.0;
            let rim = DIRS4.iter().any(|(dx, dy)| !footprint(gx + dx, gy + dy));
            world.set_tile(Pos::new(gx, gy, KEEL), Tile::Wall, wood);
            for z in KEEL + 1..=DECK {
                if rim {
                    world.set_tile(Pos::new(gx, gy, z), Tile::Wall, wood);
                } else if z == DECK {
                    world.set_tile(Pos::new(gx, gy, z), Tile::Floor, wood);
                }
            }
        }
    }
    // The main hatch, a ladder's width, over the hold.
    world.set_tile(Pos::new(8, 20, DECK), Tile::StairDown, wood);
    let mut things = Things::new();
    let gun = things.add_kind(ThingKind::new("gun", '=', Rgb(70, 70, 76), Layer::Furniture).blocking());
    let mast = things.add_kind(ThingKind::new("mast", 'O', Rgb(214, 170, 106), Layer::Furniture).blocking());
    let hand = things.add_kind(ThingKind::new("hand", '@', Rgb(230, 220, 200), Layer::Creature));
    let wheel = things.add_kind(ThingKind::new("wheel", '*', Rgb(190, 140, 80), Layer::Furniture));
    let cask = things.add_kind(ThingKind::new("cask", '0', Rgb(150, 110, 70), Layer::Item));
    for gy in (12..36).step_by(4) {
        things.spawn(gun, Pos::new(5, gy, DECK));
        things.spawn(gun, Pos::new(12, gy, DECK));
        things.spawn(hand, Pos::new(6, gy + 1, DECK));
        things.spawn(hand, Pos::new(11, gy + 1, DECK));
    }
    things.spawn(mast, Pos::new(8, 13, DECK));
    things.spawn(mast, Pos::new(8, 25, DECK));
    things.spawn(wheel, Pos::new(8, 36, DECK));
    things.spawn(hand, Pos::new(8, 37, DECK));
    for gy in (24..34).step_by(2) {
        for gx in [6, 7, 10, 11] {
            things.spawn(cask, Pos::new(gx, gy, 2));
        }
    }
    let mut scene = Scene::new(world, things, Clock::new(SECS, 10.0), seed);
    scene.sim.heat = false;
    let spec = HullSpec { mass: area * 2.5 * 1025.0, gm: 1.1, water, fouling: 1.0 };
    let mut c = Craft::new(name, scene, spec, x, y, heading);
    c.foils.push(Foil::square_sail("fore", [8.5, 13.5, 16.0], 240.0));
    c.foils.push(Foil::square_sail("main", [8.5, 25.5, 17.0], 260.0));
    c.foils.push(Foil::fore_and_aft("jib", [8.5, 2.0, 10.0], 70.0));
    c.foils.push(Foil::rudder([8.5, 39.5, 2.5], 7.0));
    c
}

/// Bathymetry: open water 60 m deep, a high island to the north-west, a low
/// one to the east with a strait between, and a bank off the big island.
fn depth(x: f32, y: f32) -> f32 {
    let isle = |cx: f32, cy: f32, r: f32, h: f32| {
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r;
        if d < 1.0 { h * (1.0 - d * d) } else { 0.0 }
    };
    let land = isle(2600.0, 3600.0, 1100.0, 90.0).max(isle(5200.0, 3300.0, 900.0, 40.0));
    let bank = isle(3500.0, 2600.0, 500.0, 50.0);
    let shelf = 60.0 - bank;
    if land > 0.0 {
        let coast = 1.0 - (land / 8.0).min(1.0);
        -land + coast * 12.0 - 2.0
    } else {
        shelf
    }
}

#[inline]
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Brace the square yards so their faces sit halfway between the wind and
/// the bow, no sharper than a square-rigger's yards will go.
fn trim(c: &mut Craft, wind: (f32, f32)) {
    let (fwd, stb) = c.axes();
    let (ax, ay) = (wind.0 - c.vx, wind.1 - c.vy);
    let m = ax.hypot(ay).max(1e-3);
    let (nx, ny) = (ax / m + fwd.0, ay / m + fwd.1);
    let mut a = (-(nx * fwd.0 + ny * fwd.1)).atan2(nx * stb.0 + ny * stb.1);
    if a > PI / 2.0 {
        a -= PI;
    } else if a <= -PI / 2.0 {
        a += PI;
    }
    let a = if a >= 0.0 { a.clamp(0.95, PI / 2.0) } else { a.clamp(-PI / 2.0, -0.95) };
    for f in c.foils.iter_mut().filter(|f| f.name == "fore" || f.name == "main") {
        f.angle = a;
    }
}

/// A flash, a splash or a puff of smoke where something happened (sea space).
#[derive(Clone, Copy)]
struct Mark {
    at: V3,
    ch: char,
    fg: Rgb,
    ttl: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Ship,
    Chart,
    Battle,
}

struct App {
    sea: Sea,
    ships: Vec<Craft>,
    shots: Ballistics,
    view: MapView,
    eview: MapView,
    screen: Screen,
    north_up: bool,
    mpc: f32,
    wind: (f32, f32),
    aloft: bool,
    smoke: Vec<(Murk, f32)>,
    marks: Vec<Mark>,
    log: Vec<(f64, String)>,
    speed: f64,
    paused: bool,
    helm: f32,
    /// Which ships the captains con (the player's own unless `--auto`).
    auto: [bool; 2],
    trim: bool,
    reload: [f32; 2],
    hits: [u32; 2],
}

impl App {
    fn new() -> App {
        let settings = SeaSettings { res: 16.0, tide_range: 2.5, tidal_stream: (0.5, 0.25), steady: (0.08, 0.0), eddy_threshold: 0.7, seed: 7, ..Default::default() };
        let mut sea = Sea::new(512, 512, settings, depth);
        sea.add_whirlpool(Whirlpool::new(3900.0, 3500.0, 45.0, 3.5, -1.0));
        let wind = (2.0, -9.0);
        sea.set_time(10.0 * 3600.0);
        sea.settle(wind);
        let mut ships = vec![brig("Dauntless", 3000.0, 6400.0, 0.5, 1), brig("Vigilante", 3950.0, 5750.0, 4.7, 2)];
        for s in &mut ships {
            s.z = sea.tide();
        }
        let mut view = MapView::new();
        view.center = Pos::new(9, 22, DECK);
        view.wide = true;
        view.lighting = false;
        let mut eview = MapView::new();
        eview.center = Pos::new(9, 22, 3);
        eview.wide = true;
        eview.lighting = false;
        App {
            sea,
            ships,
            shots: Ballistics::new(5),
            view,
            eview,
            screen: Screen::Ship,
            north_up: true,
            mpc: 8.0,
            wind,
            aloft: false,
            smoke: Vec::new(),
            marks: Vec::new(),
            log: Vec::new(),
            speed: 4.0,
            paused: false,
            helm: 0.0,
            auto: [false, true],
            trim: true,
            reload: [0.0, 20.0],
            hits: [0; 2],
        }
    }

    fn clock(&self) -> String {
        self.ships[0].scene.clock.label()
    }

    fn say(&mut self, s: String) {
        self.log.push((self.sea.time, s));
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    /// Fire every gun on one side (+1 starboard, -1 port), laid for the
    /// other ship's range if she is on that side.
    fn broadside(&mut self, from: usize, side: f32) {
        let me = &self.ships[from];
        let other = &self.ships[1 - from];
        let (dx, dy) = (other.x - me.x, other.y - me.y);
        let range = (dx * dx + dy * dy).sqrt();
        let (_, stb) = me.axes();
        let on_side = (dx * stb.0 + dy * stb.1) * side > 0.0;
        // Lead her by her way over the time of flight (the ball averages ~80% of its muzzle speed).
        let tof = range / (GUN_SPEED * 0.8);
        let (lx, ly) = (other.x + other.vx * tof, other.y + other.vy * tof);
        // "Between wind and water": at her waterline, where a hit lets the sea in.
        let mark_z = self.sea.tide() + 0.3;
        let place = me.placement();
        let guns: Vec<Pos> = me
            .scene
            .things
            .iter()
            .filter(|(_, t)| me.scene.things.kind_of(t).name == "gun")
            .map(|(_, t)| t.pos())
            .filter(|p| ((p.x as f32 + 0.5) - me.pivot.0) * side > 0.0)
            .collect();
        let (vx, vy) = (me.vx, me.vy);
        let mut fired = 0;
        let mut rng = Rng::new((self.sea.time * 7.0) as u64 ^ from as u64);
        for g in guns {
            let muzzle = place.to_world([g.x as f32 + 0.5 + side * 1.6, g.y as f32 + 0.5, DECK as f32 + 0.8]);
            // The direction he would like, in sea space...
            let world = if on_side {
                let (hx, hy) = (lx - muzzle[0], ly - muzzle[1]);
                let dist = hx.hypot(hy).max(1.0);
                let e = elevation_for(BALL, GUN_SPEED, dist, muzzle[2] - mark_z).unwrap_or(0.15);
                [hx / dist * e.cos(), hy / dist * e.cos(), e.sin()]
            } else {
                place.dir_to_world([side, 0.0, 0.02])
            };
            // ...in his gun's frame, as far as the carriage allows: 12 degrees
            // of training either way of the beam, 7 down to 12 up. The roll is
            // allowed for by eye, so it shows as a little scatter.
            let l = place.dir_to_local(world);
            let train = l[1].atan2(l[0] * side).clamp(-0.21, 0.21) + (rng.f32() - 0.5) * 0.008;
            let elev = l[2].clamp(-1.0, 1.0).asin().clamp(-0.12, 0.21) + (rng.f32() - 0.5) * 0.008;
            let local = [side * elev.cos() * train.cos(), elev.cos() * train.sin(), elev.sin()];
            let d = place.dir_to_world(local);
            let v = [vx + d[0] * GUN_SPEED, vy + d[1] * GUN_SPEED, d[2] * GUN_SPEED];
            self.shots.fire(Shot::ball(BALL, muzzle, v).from(from));
            self.smoke.push((Murk { x: muzzle[0] + d[0] * 12.0, y: muzzle[1] + d[1] * 12.0, radius: 10.0, top: 20.0, visibility: 40.0 }, 80.0));
            self.marks.push(Mark { at: muzzle, ch: '*', fg: Rgb(255, 214, 120), ttl: 1.0 });
            fired += 1;
        }
        let name = self.ships[from].name.clone();
        self.say(format!("{name} fires {fired} guns to {} at {:.0} m", if side > 0.0 { "starboard" } else { "port" }, range));
    }

    /// A captain who closes on the other ship, brings her onto his starboard
    /// beam, and fires as his guns bear and are loaded.
    fn captain(&mut self, i: usize) {
        let (me, foe) = (&self.ships[i], &self.ships[1 - i]);
        let (dx, dy) = (foe.x - me.x, foe.y - me.y);
        let range = dx.hypot(dy);
        let bearing = dx.atan2(-dy);
        let side = 1.0;
        // Far off, steer toward her; close in, put her abeam.
        let k = ((1400.0 - range) / 900.0).clamp(0.0, 1.0);
        let ideal = bearing - side * (PI / 2.0) * k;
        // The nearest course to it that she can lay (nothing within 70
        // degrees of the wind) with clear water for 400 m ahead.
        let from_wind = self.wind.0.atan2(-self.wind.1) + PI;
        let sea = &self.sea;
        let clear = |h: f32| (1..=16).all(|s| sea.water_depth(me.x + h.sin() * 25.0 * s as f32, me.y - h.cos() * 25.0 * s as f32) > 6.0);
        let mut want = ideal;
        for k in 0..13 {
            let step = 15f32.to_radians() * (((k + 1) / 2) as f32) * if k % 2 == 0 { 1.0 } else { -1.0 };
            let h = ideal + step;
            if wrap(h - from_wind).abs() >= 70f32.to_radians() && clear(h) {
                want = h;
                break;
            }
        }
        let err = wrap(want - me.heading);
        let yaw = me.yaw_rate;
        let rel = wrap(bearing - me.heading);
        let ready = self.reload[i] <= 0.0;
        self.ships[i].helm((1.4 * err - 4.0 * yaw).clamp(-0.6, 0.6));
        if ready && range < 900.0 && (rel - side * PI / 2.0).abs() < 11f32.to_radians() {
            self.broadside(i, side);
            self.reload[i] = RELOAD;
        }
    }

    fn lookout(&self) -> sight::Seen {
        let (me, other) = (&self.ships[0], &self.ships[1]);
        let mut air = Air { visibility: 18_000.0, light: me.scene.clock.daylight().sun, sea_level: self.sea.tide(), waves: self.sea.state().hs, ..Air::default() };
        air.murk = self.smoke.iter().map(|(m, _)| *m).collect();
        let eye = (me.x, me.y, self.sea.tide() + if self.aloft { 24.0 } else { 4.5 });
        let tide = self.sea.tide();
        let t = Target { x: other.x, y: other.y, bottom: tide, top: tide + 30.0, hull: 4.0, breadth: 32.0, contrast: 0.7 };
        sight::look(eye, &t, &air, if self.aloft { 1.0 } else { 7.0 }, |x, y| self.sea.ground(x, y))
    }

    fn compass(rad: f32) -> String {
        format!("{:03.0}", rad.rem_euclid(TAU).to_degrees())
    }

    fn report(seen: &sight::Seen) -> String {
        if seen.showing <= 0.0 {
            "nothing in sight".to_string()
        } else if seen.clarity < 1.0 {
            "nothing he can make out".to_string()
        } else {
            let what = if seen.hull_down { "a sail, hull down" } else if seen.clarity > 8.0 { "a brig, hull up" } else { "a ship" };
            format!("{what}, bearing {}, {:.1} km", Self::compass(seen.bearing), seen.range / 1000.0)
        }
    }

    fn ship_lines(&self, i: usize) -> Vec<(String, Rgb)> {
        let (fg, dim, warn) = (Rgb(220, 214, 196), Rgb(150, 146, 132), Rgb(240, 120, 90));
        let c = &self.ships[i];
        let mut v = vec![
            (format!("{}  {} {:.1} kn", c.name, Self::compass(c.heading), c.speed() * 1.944), fg),
            (format!(" roll {:+.0}  draft {:.2} m  water {:.1} t", c.roll.to_degrees(), c.draft(), c.water_inside / 1000.0), dim),
        ];
        let state = match c.state {
            Floating::Afloat => format!(" struck {} times, {} holes leaking", self.hits[i], c.holes.len()),
            Floating::Aground => " AGROUND".into(),
            Floating::Foundered => " FOUNDERING".into(),
        };
        v.push((state, if c.holes.is_empty() && c.state == Floating::Afloat { dim } else { warn }));
        v
    }

    fn draw_panel(&self, f: &mut Frame, r: Rect, keys: bool) {
        let (fg, dim, bg) = (Rgb(220, 214, 196), Rgb(150, 146, 132), Rgb(14, 16, 22));
        f.fill(r, Cell::new(' ', fg, bg));
        let mut lines: Vec<(String, Rgb)> = Vec::new();
        let me = &self.ships[0];
        let st = self.sea.state();
        let (rising, springs) = self.sea.tide_phase();
        let wind_sp = self.wind.0.hypot(self.wind.1);
        let cur = self.sea.current(me.x, me.y);
        lines.push((self.clock(), fg));
        lines.push((format!("wind from {} {:.0} kn; sea {:.1} m, {:.0} s", tachyon::weather::wind_from(self.wind.0, self.wind.1), wind_sp * 1.944, st.hs, st.tp), fg));
        lines.push((format!("tide {:+.2} m {} ({:.0}% springs)", self.sea.tide(), if rising { "rising" } else { "falling" }, springs * 100.0), dim));
        lines.push((format!("stream {:.1} kn to {}; whirlpools {}", cur.0.hypot(cur.1) * 1.944, Self::compass(cur.0.atan2(-cur.1)), self.sea.whirlpools.iter().filter(|w| w.now > 0.2).count()), dim));
        lines.push((String::new(), fg));
        for i in 0..2 {
            lines.extend(self.ship_lines(i));
        }
        lines.push((format!(" helm {:+.0} deg, sail {:.0}%, yards {:.0} deg off square", me.foils[3].angle.to_degrees() * -1.0, me.foils[1].set * 100.0, (PI / 2.0 - me.foils[1].angle.abs()).to_degrees()), dim));
        let seen = self.lookout();
        lines.push((format!("lookout ({}): {}", if self.aloft { "aloft" } else { "glass" }, Self::report(&seen)), fg));
        lines.push((String::new(), fg));
        if keys {
            for k in ["a/d A/D helm, x amidships; w/s sail", "[ ] brace (t: auto trim), q/e broadside", "Tab ship/chart/battle, +/- zoom, r up", "o aloft, c captain, space, < > speed, Esc"] {
                lines.push((k.into(), dim));
            }
            lines.push((String::new(), fg));
        }
        let room = (r.h as usize).saturating_sub(lines.len());
        let start = self.log.len().saturating_sub(room);
        for (_, s) in &self.log[start..] {
            lines.push((s.clone(), Rgb(214, 196, 140)));
        }
        for (k, (s, c)) in lines.iter().enumerate().take(r.h as usize) {
            f.text_clip(r.x + 1, r.y + k as i32, r.w - 2, s, *c, bg);
        }
    }

    /// Shot, splashes, flashes and smoke over a ship view.
    fn draw_effects_on_ship(&self, f: &mut Frame, area: Rect, view: &MapView, place: &Placement) {
        let (cols, rows) = view.cells_for(area);
        let (x0, y0) = (view.center.x - cols / 2, view.center.y - rows / 2);
        let mut put = |p: V3, ch: char, fg: Rgb, smoke: bool| {
            let l = place.to_local(p);
            let (sx, sy) = (l[0].floor() as i32 - x0, l[1].floor() as i32 - y0);
            if sx < 0 || sy < 0 || sx >= cols || sy >= rows {
                return;
            }
            let px = area.x + if view.wide { sx * 2 } else { sx };
            let old = f.get(px, area.y + sy);
            if smoke {
                let bg = old.bg.lerp(Rgb(170, 170, 162), 0.6);
                f.set(px, area.y + sy, Cell::new(old.ch, old.fg.lerp(bg, 0.5), bg));
                if view.wide {
                    let o2 = f.get(px + 1, area.y + sy);
                    f.set(px + 1, area.y + sy, Cell::new(o2.ch, o2.fg.lerp(bg, 0.5), bg));
                }
            } else {
                f.set(px, area.y + sy, Cell::new(ch, fg, old.bg));
            }
        };
        for (m, ttl) in &self.smoke {
            let r = m.radius.min(12.0) as i32;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy <= r * r && (dx * 7 + dy * 13 + (*ttl as i32)) % 3 != 0 {
                        put([m.x + dx as f32, m.y + dy as f32, 3.0], ' ', Rgb::WHITE, true);
                    }
                }
            }
        }
        for m in &self.marks {
            put(m.at, m.ch, m.fg, false);
        }
        for s in &self.shots.shots {
            put(s.pos, if s.in_water { 'o' } else { '*' }, Rgb(255, 240, 200), false);
        }
    }

    /// The same over a chart.
    #[allow(clippy::too_many_arguments)]
    fn draw_effects_on_chart(&self, f: &mut Frame, area: Rect, cx: f32, cy: f32, mpc: f32, up: f32, wide: bool) {
        let cols = if wide { area.w / 2 } else { area.w };
        let (right, down) = screen_axes(up);
        let mut put = |x: f32, y: f32, ch: char, fg: Rgb| {
            let (rx, ry) = (x - cx, y - cy);
            let sx = cols / 2 + ((rx * right.0 + ry * right.1) / mpc).round() as i32;
            let sy = area.h / 2 + ((rx * down.0 + ry * down.1) / mpc).round() as i32;
            if sx >= 0 && sy >= 0 && sx < cols && sy < area.h {
                let px = area.x + if wide { sx * 2 } else { sx };
                let bg = f.get(px, area.y + sy).bg;
                f.set(px, area.y + sy, Cell::new(ch, fg, bg));
            }
        };
        for (m, _) in &self.smoke {
            put(m.x, m.y, ':', Rgb(200, 200, 190));
        }
        for m in &self.marks {
            put(m.at[0], m.at[1], m.ch, m.fg);
        }
        for s in &self.shots.shots {
            put(s.pos[0], s.pos[1], '.', Rgb(255, 240, 200));
        }
    }

    fn title(f: &mut Frame, x: i32, y: i32, w: i32, s: &str) {
        let bg = Rgb(28, 30, 40);
        f.fill(Rect::new(x, y, w, 1), Cell::new(' ', Rgb(220, 214, 196), bg));
        f.text_clip(x + 1, y, w - 2, s, Rgb(232, 210, 150), bg);
    }

    fn draw_ship(&mut self, f: &mut Frame, area: Rect, i: usize) {
        let (a, b) = self.ships.split_at(1);
        let (me, other) = if i == 0 { (&a[0], &b[0]) } else { (&b[0], &a[0]) };
        let view = if i == 0 { &mut self.view } else { &mut self.eview };
        craft::draw(f, area, view, me, &self.sea, &[other]);
        let place = me.placement();
        let view = if i == 0 { &self.view } else { &self.eview };
        self.draw_effects_on_ship(f, area, view, &place);
    }

    fn draw_chart(&self, f: &mut Frame, area: Rect, mpc: f32) {
        let me = &self.ships[0];
        let up = if self.north_up { 0.0 } else { me.heading };
        let refs: Vec<&Craft> = self.ships.iter().collect();
        // Centre between the two ships when they are close, else on her.
        let (ox, oy) = (self.ships[1].x, self.ships[1].y);
        let (cx, cy) = if (ox - me.x).hypot(oy - me.y) < mpc * area.h as f32 * 0.8 { ((me.x + ox) / 2.0, (me.y + oy) / 2.0) } else { (me.x, me.y) };
        craft::draw_chart(f, area, &self.sea, &refs, cx, cy, mpc, up, true);
        self.draw_effects_on_chart(f, area, cx, cy, mpc, up, true);
    }

    /// Both ships and the chart: her deck, the enemy's hold, the chart and the state of things.
    fn draw_battle(&mut self, f: &mut Frame) {
        let (w, h) = (f.width(), f.height());
        let sw = 40;
        Self::title(f, 0, 0, sw, &format!("{} - deck", self.ships[0].name));
        self.draw_ship(f, Rect::new(0, 1, sw, h - 1), 0);
        Self::title(f, sw, 0, sw, &format!("{} - hold (level {})", self.ships[1].name, self.eview.center.z));
        self.draw_ship(f, Rect::new(sw, 1, sw, h - 1), 1);
        let rx = sw * 2;
        let ch = (h * 11 / 20).max(10);
        let bmpc = (self.mpc * 4.0).max(24.0);
        Self::title(f, rx, 0, w - rx, &format!("chart, {bmpc:.0} m a cell, north up"));
        self.draw_chart(f, Rect::new(rx, 1, w - rx, ch - 1), bmpc);
        self.draw_panel(f, Rect::new(rx, ch, w - rx, h - ch), false);
    }
}

impl Game for App {
    fn tick(&mut self) {
        let dt = SECS as f32;
        // The breeze freshens over the first hour.
        let t = self.sea.time as f32 / 3600.0 - 10.0;
        let s = (9.0 + 3.0 * (t * 0.8).min(1.0)) / 9.0;
        let wind = (2.0 * s, -9.0 * s);
        self.wind = wind;
        self.sea.step(dt, wind);
        for i in 0..2 {
            self.reload[i] -= dt;
            if self.auto[i] {
                self.captain(i);
            }
            if self.trim || i == 1 {
                trim(&mut self.ships[i], wind);
            }
        }
        for c in &mut self.ships {
            c.step(&self.sea, wind);
        }
        craft::collide(&mut self.ships);
        {
            let mut bodies: Vec<Body> = self.ships.iter_mut().enumerate().map(|(i, c)| c.body(i)).collect();
            self.shots.step(dt, Some(&self.sea), wind, &mut bodies);
        }
        let events = std::mem::take(&mut self.shots.events);
        for (i, c) in self.ships.iter_mut().enumerate() {
            c.take_hits(i, &events, &self.sea);
        }
        let mut hits = [0u32; 2];
        let mut killed = [0u32; 2];
        for e in &events {
            match e {
                ShotEvent::Struck { body, cell, .. } => {
                    hits[*body] += 1;
                    let at = self.ships[*body].placement().cell_to_world(*cell);
                    self.marks.push(Mark { at, ch: '#', fg: Rgb(255, 150, 60), ttl: 3.0 });
                }
                ShotEvent::HitThing { body, thing, cell, .. } => {
                    let things = &mut self.ships[*body].scene.things;
                    if things.get(*thing).is_some_and(|t| things.kind_of(t).layer == Layer::Creature) {
                        things.despawn(*thing);
                        killed[*body] += 1;
                    }
                    let at = self.ships[*body].placement().cell_to_world(*cell);
                    self.marks.push(Mark { at, ch: '%', fg: Rgb(230, 60, 50), ttl: 4.0 });
                }
                ShotEvent::Splash { at, .. } => self.marks.push(Mark { at: *at, ch: 'o', fg: Rgb(225, 240, 250), ttl: 3.0 }),
                ShotEvent::Ricochet { at, .. } => self.marks.push(Mark { at: *at, ch: '^', fg: Rgb(225, 240, 250), ttl: 2.0 }),
                _ => {}
            }
        }
        for i in 0..2 {
            self.hits[i] += hits[i];
            if hits[i] > 0 || killed[i] > 0 {
                let n = self.ships[i].name.clone();
                let men = if killed[i] > 0 { format!(", {} men down", killed[i]) } else { String::new() };
                self.say(format!("{n} struck {} times{men}", hits[i]));
            }
        }
        let mut msgs = Vec::new();
        for c in &mut self.ships {
            for e in c.events.drain(..) {
                match e {
                    CraftEvent::Grounded { speed, .. } => msgs.push(format!("{} takes the ground at {:.1} kn!", c.name, speed * 1.944)),
                    CraftEvent::Refloated => msgs.push(format!("{} floats off", c.name)),
                    CraftEvent::Collided { impulse, .. } if impulse > 1e4 => msgs.push(format!("{} collides!", c.name)),
                    CraftEvent::Foundered => msgs.push(format!("{} is going down!", c.name)),
                    CraftEvent::Holed { below_water: true, .. } => msgs.push(format!("{} holed below the waterline", c.name)),
                    _ => {}
                }
            }
        }
        for m in msgs {
            self.say(m);
        }
        // Powder smoke drifts downwind, spreads and thins.
        for (m, ttl) in &mut self.smoke {
            m.x += wind.0 * dt;
            m.y += wind.1 * dt;
            m.radius += 0.25 * dt;
            *ttl -= dt;
        }
        self.smoke.retain(|(_, t)| *t > 0.0);
        for m in &mut self.marks {
            m.ttl -= dt;
        }
        self.marks.retain(|m| m.ttl > 0.0);
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        match k.key {
            Key::Esc => return Flow::Quit,
            Key::Tab => {
                self.screen = match self.screen {
                    Screen::Ship => Screen::Chart,
                    Screen::Chart => Screen::Battle,
                    Screen::Battle => Screen::Ship,
                }
            }
            Key::PageUp => self.view.center.z = (self.view.center.z + 1).min(9),
            Key::PageDown => self.view.center.z = (self.view.center.z - 1).max(0),
            Key::Char(c) => match c {
                'a' => self.helm = (self.helm - 0.08).max(-0.6),
                'd' => self.helm = (self.helm + 0.08).min(0.6),
                'A' => self.helm = -0.6,
                'D' => self.helm = 0.6,
                'x' => self.helm = 0.0,
                'w' | 's' => {
                    for f in self.ships[0].foils.iter_mut().filter(|f| f.medium == Medium::Air) {
                        f.set = (f.set + if c == 'w' { 0.25 } else { -0.25 }).clamp(0.0, 1.0);
                    }
                }
                '[' | ']' => {
                    self.trim = false;
                    for f in self.ships[0].foils.iter_mut().filter(|f| f.name == "fore" || f.name == "main") {
                        let a = f.angle + if c == '[' { -0.1 } else { 0.1 };
                        f.angle = if a > 0.0 { a.clamp(0.95, PI / 2.0) } else { a.clamp(-PI / 2.0, -0.95) };
                    }
                }
                't' => self.trim = !self.trim,
                'c' => self.auto[0] = !self.auto[0],
                'q' if self.reload[0] <= 0.0 => {
                    self.broadside(0, -1.0);
                    self.reload[0] = RELOAD;
                }
                'e' if self.reload[0] <= 0.0 => {
                    self.broadside(0, 1.0);
                    self.reload[0] = RELOAD;
                }
                '+' | '=' => self.mpc = (self.mpc / 2.0).max(1.0),
                '-' => self.mpc = (self.mpc * 2.0).min(256.0),
                'r' => self.north_up = !self.north_up,
                'o' => self.aloft = !self.aloft,
                ' ' => self.paused = !self.paused,
                '>' => self.speed = (self.speed * 2.0).min(64.0),
                '<' => self.speed = (self.speed / 2.0).max(1.0),
                _ => {}
            },
            _ => {}
        }
        if !self.auto[0] {
            let h = self.helm;
            self.ships[0].helm(h);
        }
        Flow::Continue
    }

    fn draw(&mut self, f: &mut Frame, _stats: &LoopStats) {
        if self.screen == Screen::Battle {
            self.draw_battle(f);
            return;
        }
        let pw = 44.min(f.width() / 3);
        let main = Rect::new(0, 0, f.width() - pw, f.height());
        if self.screen == Screen::Chart {
            self.draw_chart(f, main, self.mpc);
        } else {
            self.draw_ship(f, main, 0);
        }
        self.draw_panel(f, Rect::new(main.right(), 0, pw, f.height()), true);
    }

    fn ticks_per_second(&self) -> f64 {
        if self.paused { 0.0 } else { self.speed / SECS }
    }
}

// ------------------------------------------------------------------ replay

/// Frames recorded for the replay page: each row is runs of (fg, bg, text),
/// colours indexed into one shared palette and rounded to steps of 4 so
/// runs stay long.
struct Recorder {
    palette: Vec<Rgb>,
    index: std::collections::HashMap<Rgb, usize>,
    frames: String,
    count: usize,
}

impl Recorder {
    fn new() -> Recorder {
        Recorder { palette: Vec::new(), index: Default::default(), frames: String::new(), count: 0 }
    }

    fn colour(&mut self, c: Rgb) -> usize {
        let q = Rgb(c.0 & 0xFC, c.1 & 0xFC, c.2 & 0xFC);
        if let Some(&i) = self.index.get(&q) {
            return i;
        }
        self.palette.push(q);
        self.index.insert(q, self.palette.len() - 1);
        self.palette.len() - 1
    }

    fn add(&mut self, f: &Frame, time: &str, log: &[&str]) {
        if self.count > 0 {
            self.frames.push(',');
        }
        self.count += 1;
        let _ = write!(self.frames, "{{\"t\":{},\"log\":[", json(time));
        for (k, l) in log.iter().enumerate() {
            if k > 0 {
                self.frames.push(',');
            }
            self.frames.push_str(&json(l));
        }
        self.frames.push_str("],\"rows\":[");
        for y in 0..f.height() {
            if y > 0 {
                self.frames.push(',');
            }
            self.frames.push('[');
            let mut run: Option<(usize, usize, String)> = None;
            let mut first = true;
            for x in 0..f.width() {
                let c = f.get(x, y);
                let (fi, bi) = (self.colour(c.fg), self.colour(c.bg));
                // A blank cell takes any foreground, so it extends the run.
                let blank = c.ch == ' ';
                match &mut run {
                    Some((rf, rb, s)) if *rb == bi && (*rf == fi || blank) => s.push(c.ch),
                    _ => {
                        if let Some((rf, rb, s)) = run.take() {
                            if !first {
                                self.frames.push(',');
                            }
                            first = false;
                            let _ = write!(self.frames, "{rf},{rb},{}", json(&s));
                        }
                        run = Some((fi, bi, c.ch.to_string()));
                    }
                }
            }
            if let Some((rf, rb, s)) = run {
                if !first {
                    self.frames.push(',');
                }
                let _ = write!(self.frames, "{rf},{rb},{}", json(&s));
            }
            self.frames.push(']');
        }
        self.frames.push_str("]}");
    }

    fn page(&self, w: i32, h: i32, summary: &str) -> String {
        let pal: Vec<String> = self.palette.iter().map(|c| format!("\"#{:02x}{:02x}{:02x}\"", c.0, c.1, c.2)).collect();
        PAGE.replace("{{W}}", &w.to_string())
            .replace("{{H}}", &h.to_string())
            .replace("{{SUMMARY}}", summary)
            .replace("{{PALETTE}}", &pal.join(","))
            .replace("{{FRAMES}}", &self.frames)
    }
}

fn json(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '<' => o.push_str("\\u003c"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

const PAGE: &str = include_str!("replay.html");

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let t = std::time::Instant::now();
    let mut app = App::new();
    let built = t.elapsed();
    if flag("--auto") {
        app.auto = [true, true];
    }

    if let Some(path) = arg("--replay") {
        app.auto = [true, true];
        let ticks: usize = arg("--ticks").and_then(|s| s.parse().ok()).unwrap_or(1800);
        let every: usize = arg("--every").and_then(|s| s.parse().ok()).unwrap_or(6);
        let (fw, fh) = (150u16, 46u16);
        let mut rec = Recorder::new();
        let mut f = Frame::new(fw, fh);
        let mut seen_log = 0usize;
        let t = std::time::Instant::now();
        for k in 0..=ticks {
            if k > 0 {
                app.tick();
            }
            if k % every == 0 {
                app.draw_battle(&mut f);
                let new: Vec<&str> = app.log.iter().skip(seen_log.min(app.log.len())).map(|(_, s)| s.as_str()).collect();
                seen_log = app.log.len();
                let time = app.clock();
                rec.add(&f, &time, &new);
            }
        }
        let [a, b] = [&app.ships[0], &app.ships[1]];
        let summary = format!(
            "{} struck {} times, {:.0} t of water in her; {} struck {} times, {:.0} t of water in her.",
            a.name,
            app.hits[0],
            a.water_inside / 1000.0,
            b.name,
            app.hits[1],
            b.water_inside / 1000.0
        );
        let page = rec.page(fw as i32, fh as i32, &json(&summary));
        std::fs::write(&path, &page)?;
        println!(
            "{} frames over {:.0} game minutes in {:.1} s; {} colours; {:.1} MB -> {path}\n{summary}",
            rec.count,
            ticks as f64 * SECS / 60.0,
            t.elapsed().as_secs_f64(),
            rec.palette.len(),
            page.len() as f64 / 1e6
        );
        for (_, l) in &app.log {
            println!("  {l}");
        }
        return Ok(());
    }

    if let Some(path) = arg("--snapshot") {
        let ticks: usize = arg("--ticks").and_then(|s| s.parse().ok()).unwrap_or(120);
        app.screen = if flag("--chart") { Screen::Chart } else if flag("--battle") { Screen::Battle } else { Screen::Ship };
        app.aloft = flag("--aloft");
        if let Some(m) = arg("--mpc").and_then(|s| s.parse().ok()) {
            app.mpc = m;
        }
        let t = std::time::Instant::now();
        for k in 0..ticks {
            if flag("--fire") && k == ticks.saturating_sub(4) {
                app.broadside(0, 1.0);
            }
            app.tick();
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / ticks.max(1) as f64;
        let mut f = Frame::new(170, 52);
        let t = std::time::Instant::now();
        app.draw(&mut f, &LoopStats::default());
        let draw = t.elapsed().as_secs_f64() * 1000.0;
        std::fs::write(&path, f.to_html())?;
        std::fs::write(format!("{path}.txt"), f.to_text())?;
        println!("sea built in {:.0} ms; {ticks} ticks at {per:.2} ms; draw {draw:.1} ms; wrote {path}", built.as_secs_f64() * 1000.0);
        for (_, l) in &app.log {
            println!("  {l}");
        }
        return Ok(());
    }
    tachyon::app::run(&mut app, 60)
}
