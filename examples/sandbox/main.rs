//! Tachyon sandbox: walk around a test map and poke at the physics.
//!
//!     cargo run --release --example sandbox
//!     cargo run --release --example sandbox -- --size 512x512x16      (stress test)
//!     cargo run --release --example sandbox -- --bench 600             (headless timing)
//!     cargo run --release --example sandbox -- --snapshot frame.html   (render one frame to a file)

mod mapgen;

use mapgen::{Built, Ids, G};
use std::collections::VecDeque;
use std::io;
use std::time::Instant;
use tachyon::frame::Cell;
use tachyon::prelude::*;
use tachyon::sim::flames::{self, Lighting};
use tachyon::view::inspect::describe;
use tachyon::weather::{describe_wind, wind_from};

const SPEEDS: [f64; 6] = [2.0, 5.0, 10.0, 20.0, 50.0, 100.0];
/// Prevailing winds `W` cycles through: speed (cells per tick) and the
/// direction it blows toward (radians; 0 east, PI/2 south). Gusts and
/// wander come from the weather.
const WINDS: [(f32, f32); 6] = [
    (0.0, 0.0),
    (0.15, 0.2),
    (0.35, std::f32::consts::FRAC_PI_2),
    (0.7, std::f32::consts::PI),
    (0.5, -std::f32::consts::FRAC_PI_2),
    (1.3, 0.1),
];
/// Rain settings `R` cycles through.
const RAINS: [(&str, f32); 3] = [("dry", 0.0), ("light rain", 0.3), ("heavy rain", 1.0)];
/// The firebolt: speed in cells per tick, and its explosion.
const BOLT_SPEED: f32 = 1.0;
const BOLT: Blast = Blast { radius: 2.5, power: 120.0, heat: 300.0, ignite: 1.0 };
/// Speed a paddle stroke adds to a raft, in cells per tick.
const PADDLE: f32 = 0.025;
/// What `K` cycles through: your bare hands, then weapons (name, verb, kind, force).
const WEAPONS: [(&str, &str, DamageKind, f32); 5] = [
    ("fists", "", DamageKind::Blunt, 0.0),
    ("knife", "slash", DamageKind::Edge, 22.0),
    ("club", "club", DamageKind::Blunt, 45.0),
    ("spear", "stab", DamageKind::Point, 45.0),
    ("axe", "hack", DamageKind::Edge, 70.0),
];

struct Sandbox {
    scene: Scene,
    ids: Ids,
    view: MapView,
    player: ThingId,
    /// Look cursor (also the target of the tools).
    look: Option<Pos>,
    /// Levels above/below the player the camera is looking at.
    z_offset: i32,
    see_all: bool,
    paused: bool,
    speed: usize,
    help: bool,
    log: VecDeque<String>,
    rng: Rng,
    /// Held direction keys, for walking (and moving the look cursor).
    pad: DirPad,
    wind: usize,
    rain: usize,
    /// Aiming a firebolt: the target cell. The game is paused meanwhile.
    aim: Option<Pos>,
    was_paused: bool,
    /// The next direction attacks instead of walking.
    attacking: bool,
    weapon: usize,
}

impl Sandbox {
    fn new(w: i32, h: i32, d: i32, seed: u64) -> Sandbox {
        let Built { scene, ids, player } = mapgen::build(w, h, d, seed);
        let mut s = Sandbox {
            scene,
            ids,
            view: MapView::new(),
            player,
            look: None,
            z_offset: 0,
            see_all: false,
            paused: false,
            speed: 2,
            help: true,
            log: VecDeque::new(),
            rng: Rng::new(seed ^ 99),
            pad: DirPad::new(),
            wind: 1,
            rain: 0,
            aim: None,
            was_paused: false,
            attacking: false,
            weapon: 0,
        };
        // Two characters per cell looks square in most terminal fonts.
        s.view.wide = true;
        s.set_weather();
        s.say("You stand outside a stone house. x looks around; ? hides the help.");
        s
    }

    fn set_weather(&mut self) {
        let (speed, dir) = WINDS[self.wind];
        self.scene.weather.set_wind(speed, dir);
        self.scene.weather.rain = RAINS[self.rain].1;
    }

    fn say(&mut self, m: impl Into<String>) {
        self.log.push_front(m.into());
        self.log.truncate(4);
    }

    fn player_pos(&self) -> Pos {
        self.scene.things.get(self.player).map(|t| t.pos()).unwrap_or_default()
    }

    /// Your state, if it stops you acting.
    fn incapable(&self) -> Option<&'static str> {
        let c = self.scene.life.get(self.player)?;
        if !c.alive {
            Some("You are dead. (q quits)")
        } else if c.out {
            Some("You are unconscious...")
        } else if c.mind.asleep {
            Some("You are asleep. (Z wakes you)")
        } else {
            None
        }
    }

    /// Attack whatever is in that direction.
    fn strike(&mut self, dx: i32, dy: i32) {
        self.attacking = false;
        let at = self.player_pos().offset(dx, dy, 0);
        let Some(target) = self.scene.things.creature_at(at) else {
            self.say("There's nothing there to attack.");
            return;
        };
        if self.scene.life.get(target).is_some_and(|c| !c.alive) {
            self.say("It's already dead.");
            return;
        }
        self.scene.life.attack(&self.scene.things, self.player, target);
        if self.scene.life.events.is_empty() {
            self.say("You can't attack like that.");
        }
        self.report_life();
    }

    fn set_weapon(&mut self) {
        let (name, verb, kind, force) = WEAPONS[self.weapon];
        if let Some(c) = self.scene.life.get_mut(self.player) {
            c.weapon = (force > 0.0).then(|| Attack::new(verb, &format!("{verb}es"), kind, force, Does::GRASP));
        }
        self.say(format!("You ready your {name}."));
    }

    /// Tell the player what creatures did, if they could see it.
    fn report_life(&mut self) {
        let events = std::mem::take(&mut self.scene.life.events);
        let p = self.player_pos();
        for e in &events {
            let who = e.who();
            let mine = who.contains(&self.player);
            let seen = who.iter().any(|&id| {
                let q = self.scene.things.get(id).map(|t| t.pos());
                q.is_some_and(|q| self.see_all || (self.view.visible(q) && q.dist2d(p) <= 20))
            });
            // Eating and sleeping are only news when it's you.
            let quiet = matches!(e, LifeEvent::Ate { .. } | LifeEvent::Drank(_) | LifeEvent::FellAsleep(_) | LifeEvent::Woke(_) | LifeEvent::Burned { .. });
            if mine || (seen && !quiet) {
                let line = self.scene.life.narrate(&self.scene.things, e);
                if self.log.front() != Some(&line) {
                    self.say(line);
                }
            }
        }
    }

    fn walk(&mut self, dx: i32, dy: i32) {
        if let Some(why) = self.incapable() {
            self.say(why);
            return;
        }
        if self.attacking {
            self.strike(dx, dy);
            return;
        }
        // On a raft, stepping off the edge into water paddles instead.
        let p = self.player_pos();
        if let Some(v) = self.scene.vessel_at(p) {
            let to = p.offset(dx, dy, 0);
            let (w, t) = (&self.scene.world, &self.scene.things);
            let land = w.standable(to) || t.deck_at(to) || (w.tile(to).is_wall() && w.standable(to.up()));
            if !self.scene.vessels[v].contains(to) && !land {
                let n = ((dx * dx + dy * dy) as f32).sqrt();
                let vessel = &mut self.scene.vessels[v];
                let first = vessel.speed() < 0.01;
                vessel.push(dx as f32 * PADDLE / n, dy as f32 * PADDLE / n);
                if first {
                    self.say("You paddle. (J jumps overboard.)");
                }
                return;
            }
        }
        match movement::step(&self.scene.world, &mut self.scene.things, self.player, dx, dy) {
            Step::Moved(p) => {
                self.z_offset = 0;
                let w = &self.scene.world;
                if let Some(i) = w.index(p) {
                    if w.fluid[i] >= movement::SWIM_DEPTH {
                        let name = w.mats.get(w.fluid_mat[i]).name.clone();
                        self.say(format!("You swim in the {name}."));
                    } else if w.fire[i] > 0 {
                        self.say("You are standing in fire!");
                    }
                }
            }
            Step::OpenedDoor(_) => self.say("You open the door."),
            Step::Ledge => self.say("There's a drop there. (Look first: x)"),
            Step::Blocked => {}
        }
    }

    fn tool(&mut self, c: char) {
        let Some(p) = self.look else { return };
        let ids = &self.ids;
        let w = &mut self.scene.world;
        match c {
            'w' => {
                let Some(i) = w.index(p) else { return };
                if w.tile[i].is_wall() {
                    // A wall holds no water, but you can throw it over one.
                    let out = w.quench(i);
                    self.say(if out { "You throw water over the burning wall. It hisses out." } else { "The water splashes off the wall." });
                } else {
                    let burning = w.fire[i] > 0;
                    let n = w.add_fluid(p, ids.water, FULL);
                    self.say(match (n > 0, burning) {
                        (true, true) => "You pour water on the fire. It hisses out.",
                        (true, false) => "You pour water.",
                        _ => "No room for water there.",
                    });
                }
            }
            'r' => {
                let n = w.add_fluid(p, ids.lava, FULL);
                self.say(if n > 0 { "Lava wells up." } else { "No room for lava there." });
            }
            'f' => {
                let ok = w.ignite(p);
                if !ok {
                    // Nothing flammable: heat the spot instead.
                    w.set_temp(p, 600.0);
                }
                self.say(if ok { "Fire!" } else { "Nothing there burns; you heat it to 600 C." });
            }
            's' => {
                w.add_gas(p, ids.smoke, 250);
                self.say("A cloud of smoke.");
            }
            'c' => {
                w.set_temp(p, -40.0);
                self.say("You chill it to -40 C.");
            }
            'd' => {
                if w.tile(p).is_wall() {
                    let m = w.mat[w.index(p).unwrap()];
                    w.set_tile(p, Tile::Floor, m);
                    self.say("You dig out the wall.");
                } else {
                    w.set_tile(p, Tile::Wall, ids.block);
                    self.say("You build a stone wall.");
                }
            }
            't' => {
                let torch = self.ids.torch;
                let existing = self.scene.things.at(p).find(|(_, t)| t.kind == torch).map(|(id, t)| (id, t.lit));
                match existing {
                    Some((id, true)) => {
                        self.scene.things.despawn(id);
                        self.say("You take the torch.");
                    }
                    Some((id, false)) => {
                        let msg = match flames::try_light(&self.scene.world, &mut self.scene.things, id, &mut self.rng) {
                            Lighting::Lit => "The torch catches.",
                            Lighting::Sputters => "The damp torch smokes and sputters; it won't catch.",
                            Lighting::Underwater => "You can't light a torch underwater.",
                            _ => "",
                        };
                        self.say(msg);
                    }
                    None => {
                        let id = self.scene.things.spawn(torch, p);
                        let w = &self.scene.world;
                        if w.index(p).is_some_and(|i| w.fluid[i] >= flames::DOUSE_DEPTH) {
                            if let Some(t) = self.scene.things.get_mut(id) {
                                t.lit = false;
                                t.wet = flames::SOAKED;
                            }
                            self.say("The torch hisses out in the water.");
                        } else {
                            self.say("You set down a torch.");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn draw_panel(&self, f: &mut Frame, r: Rect, stats: &LoopStats) {
        let bg = Rgb(12, 12, 16);
        f.fill(r, Cell::new(' ', Rgb::WHITE, bg));
        let (x, w) = (r.x + 2, r.w - 3);
        let mut y = r.y + 1;
        let title = Rgb(240, 200, 90);
        let txt = Rgb(200, 200, 205);
        let dim = Rgb(120, 120, 130);
        let line = |f: &mut Frame, y: &mut i32, s: &str, c: Rgb| {
            if *y < r.bottom() - 1 {
                f.text_clip(x, *y, w, s, c, bg);
            }
            *y += 1;
        };
        line(f, &mut y, "TACHYON  sandbox", title);
        let clock = &self.scene.clock;
        let run = if self.paused { "PAUSED".to_string() } else { format!("{} ticks/s", SPEEDS[self.speed]) };
        line(f, &mut y, &format!("{}   {}", clock.label(), run), txt);
        let p = self.player_pos();
        line(f, &mut y, &format!("you ({}, {}) level {}   view {}", p.x, p.y, p.z, self.view.center.z), txt);
        line(f, &mut y, &format!("sight: {}   cells: {}", if self.see_all { "everything" } else { "your eyes" }, if self.view.wide { "2 wide" } else { "1 wide" }), txt);
        line(f, &mut y, &format!("overlay: {}   light: {}", self.view.overlay.name(), if self.view.lighting { "on" } else { "off" }), txt);
        let (wx, wy) = self.scene.world.wind;
        let ws = (wx * wx + wy * wy).sqrt();
        let wind = if ws < 0.05 { "calm".to_string() } else { format!("{} from the {}", describe_wind(ws), wind_from(wx, wy)) };
        line(f, &mut y, &format!("wind: {wind}"), txt);
        line(f, &mut y, &format!("weather: {}", RAINS[self.rain].0), txt);
        for v in &self.scene.vessels {
            let state = match v.state {
                Afloat::Afloat => "afloat",
                Afloat::Touching => "scraping bottom",
                Afloat::Aground => "aground",
            };
            let o = v.origin();
            // A tick is a tenth of a second of game time.
            line(f, &mut y, &format!("{} ({}, {}) {}  {:.2} cells/tick", v.name, o.x, o.y, state, v.speed()), txt);
        }
        y += 1;
        let st = &self.scene.sim.stats;
        line(f, &mut y, &format!("{:.0} fps  draw {:.1}ms  out {:.1}ms", stats.fps, stats.draw_ms, stats.present_ms), dim);
        line(f, &mut y, &format!("tick {:.2}ms  ({:.0}/s run)", stats.tick_ms, stats.tps), dim);
        line(f, &mut y, &format!("awake {}/{} chunks", st.awake_chunks, st.total_chunks), dim);
        line(f, &mut y, &format!("key releases from: {}", stats.key_ups), dim);
        line(f, &mut y, &format!("{} cells changed, {:.1} KB out", stats.cells_changed, stats.bytes as f32 / 1024.0), dim);
        let wd = &self.scene.world;
        line(f, &mut y, &format!("world {}x{}x{} = {:.1}M cells", wd.w, wd.h, wd.d, wd.len() as f32 / 1e6), dim);
        y += 1;

        let target = self.aim.or(self.look).unwrap_or(p);
        let head = if self.aim.is_some() {
            "AIMING AT"
        } else if self.look.is_some() {
            "LOOKING AT"
        } else {
            "WHERE YOU STAND"
        };
        line(f, &mut y, head, title);
        let known = self.see_all || self.view.visible(target) || self.view.has_seen(wd, target);
        let life = &self.scene.life;
        // A creature there (or you, or a body): its state and wounds first.
        let mine = self.look.is_none() && self.aim.is_none();
        let who = if mine { Some(self.player) } else { self.scene.things.at(target).map(|(id, _)| id).find(|id| life.get(*id).is_some()) };
        if known {
            if let Some(id) = who {
                for (s, c) in life.describe(&self.scene.things, id) {
                    for (k, part) in wrap(&s, w as usize - 2).into_iter().enumerate() {
                        line(f, &mut y, &if k == 0 { part } else { format!("  {part}") }, c);
                    }
                }
                y += 1;
            }
            for s in describe(&self.scene, target) {
                line(f, &mut y, &s, txt);
            }
        } else {
            line(f, &mut y, "you can't see there", dim);
        }
        y += 1;

        if self.help {
            let keys: &[&str] = if self.aim.is_some() {
                &[
                    "AIMING (paused)",
                    "move target  arrows / hjklyubn",
                    "< >          target level up/down",
                    "Enter / f    throw the firebolt",
                    "Esc          cancel",
                ]
            } else if self.look.is_some() {
                &[
                    "LOOK MODE",
                    "move cursor  arrows / hjklyubn",
                    "< >          cursor level up/down",
                    "w water  r lava  f fire/heat",
                    "s smoke  c chill  d dig/build",
                    "t torch on/off",
                    "x / Esc      back to walking",
                ]
            } else {
                &[
                    "KEYS",
                    "move     arrows / hjklyubn / numpad",
                    "< >      climb stairs up/down",
                    "PgUp/Dn  look at other levels",
                    "x        look mode + tools",
                    "v        see everything / your eyes",
                    "o        overlay (heat, liquid ...)",
                    "m        1 or 2 chars per cell",
                    "g        lighting on/off   U hillsides",
                    "L        carry a lantern",
                    "C        close doors around you",
                    "         walk off a raft to paddle",
                    "J jump overboard",
                    "a        attack (then a direction)",
                    "K        change weapon",
                    "E eat    D drink    Z sleep",
                    "f        throw a firebolt (aim)",
                    "W wind   R rain   F fog rolls in",
                    "space    pause    + - speed",
                    "T        skip an hour",
                    "?        hide help   q quit",
                ]
            };
            for (i, s) in keys.iter().enumerate() {
                line(f, &mut y, s, if i == 0 { title } else { dim });
            }
        }
    }
}

/// Split a line at spaces so each piece fits in `width` characters.
fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    for word in s.split(' ') {
        let cur = out.last_mut().unwrap();
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(word.to_string());
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    out
}

fn dir_for(k: &KeyEvent) -> Option<(i32, i32)> {
    Some(match k.key {
        Key::Up => (0, -1),
        Key::Down => (0, 1),
        Key::Left => (-1, 0),
        Key::Right => (1, 0),
        Key::Home => (-1, -1),
        Key::End => (-1, 1),
        Key::Char(c) => match c {
            'k' | '8' => (0, -1),
            'j' | '2' => (0, 1),
            'h' | '4' => (-1, 0),
            'l' | '6' => (1, 0),
            'y' | '7' => (-1, -1),
            'u' | '9' => (1, -1),
            'b' | '1' => (-1, 1),
            'n' | '3' => (1, 1),
            _ => return None,
        },
        _ => return None,
    })
}

impl Game for Sandbox {
    fn tick(&mut self) {
        self.scene.tick();
        let events = std::mem::take(&mut self.scene.events);
        for e in events {
            match e {
                SceneEvent::Exploded(_, r) => {
                    let mut m = String::from("BOOM!");
                    if r.broken > 0 {
                        m += &format!(" {} blocks smashed.", r.broken);
                    }
                    if r.ignited > 0 {
                        m += &format!(" {} set alight.", r.ignited);
                    }
                    if r.things_destroyed > 0 {
                        m += &format!(" {} things blown apart.", r.things_destroyed);
                    }
                    self.say(m);
                }
                SceneEvent::Fizzled(_) => self.say("The firebolt hisses out in the water."),
            }
        }
        self.report_life();
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        // Walking and the look cursor go through the pad, so holding two
        // arrows moves diagonally and holding one walks steadily.
        if let Some(d) = dir_for(&k) {
            if !k.ctrl && !k.alt {
                self.pad.press(k.key, d, Instant::now());
                return Flow::Continue;
            }
        }
        // Aiming: the game waits while you pick a target.
        if let Some(cur) = self.aim {
            match k.key {
                Key::Esc => {
                    self.aim = None;
                    self.paused = self.was_paused;
                    self.pad.clear();
                    self.say("You let the flame die in your hand.");
                }
                Key::Enter | Key::Char('f') => self.throw_bolt(cur),
                Key::Char('<') | Key::PageUp => self.aim = Some(cur.up()),
                Key::Char('>') | Key::PageDown => self.aim = Some(cur.down()),
                _ => {}
            }
            return Flow::Continue;
        }
        // Look mode: the cursor moves instead of you, and tools act on it.
        if let Some(cur) = self.look {
            match k.key {
                Key::Esc | Key::Char('x') => {
                    self.look = None;
                    self.pad.clear();
                }
                Key::Char('<') | Key::PageUp => self.look = Some(cur.up()),
                Key::Char('>') | Key::PageDown => self.look = Some(cur.down()),
                Key::Char(c @ ('w' | 'r' | 'f' | 's' | 'c' | 'd' | 't')) => self.tool(c),
                _ => return self.common_key(k),
            }
            return Flow::Continue;
        }
        match k.key {
            Key::Esc if self.attacking => {
                self.attacking = false;
                self.say("You lower your guard.");
            }
            Key::Char('q') | Key::Esc => return Flow::Quit,
            Key::Char('x') => {
                let p = self.player_pos();
                self.look = Some(p.offset(0, 0, self.z_offset));
                self.pad.clear();
            }
            Key::Char('J') => self.jump_overboard(),
            Key::Char('a') => {
                if let Some(why) = self.incapable() {
                    self.say(why);
                } else {
                    self.attacking = !self.attacking;
                    self.pad.clear();
                    let w = WEAPONS[self.weapon].0;
                    self.say(if self.attacking { format!("Attack which way? (with your {w})") } else { "You lower your guard.".to_string() });
                }
            }
            Key::Char('K') => {
                self.weapon = (self.weapon + 1) % WEAPONS.len();
                self.set_weapon();
            }
            Key::Char('E') => match self.scene.life.food_in_reach(&self.scene.things, self.player) {
                Some(f) if self.incapable().is_none() => {
                    self.scene.life.eat(&mut self.scene.things, self.player, f);
                    self.report_life();
                }
                Some(_) => {}
                None => self.say("There's nothing here you can eat."),
            },
            Key::Char('D') => {
                let p = self.player_pos();
                match self.scene.life.water_near(&self.scene.world, p) {
                    Some(w) if self.incapable().is_none() => {
                        self.scene.life.drink(&mut self.scene.world, &self.scene.things, self.player, w);
                        self.report_life();
                    }
                    Some(_) => {}
                    None => self.say("There's no water within reach."),
                }
            }
            Key::Char('Z') => {
                let Some(c) = self.scene.life.get_mut(self.player) else { return Flow::Continue };
                if c.alive && !c.out {
                    c.mind.asleep = !c.mind.asleep;
                    let asleep = c.mind.asleep;
                    self.say(if asleep { "You lie down and sleep. (Z wakes you; time runs fast)" } else { "You get up." });
                }
            }
            Key::Char('f') => {
                self.aim = Some(self.player_pos());
                self.was_paused = self.paused;
                self.paused = true;
                self.pad.clear();
                self.say("A flame gathers in your hand. Pick a target; Enter throws, Esc cancels.");
            }
            Key::Char('W') => {
                self.wind = (self.wind + 1) % WINDS.len();
                self.set_weather();
                let (speed, _) = WINDS[self.wind];
                self.say(if speed == 0.0 { "The wind drops.".to_string() } else { format!("The wind changes: {}.", describe_wind(speed)) });
            }
            Key::Char('R') => {
                self.rain = (self.rain + 1) % RAINS.len();
                self.set_weather();
                self.say(format!("The weather turns: {}.", RAINS[self.rain].0));
            }
            Key::Char('F') => self.fog_rolls_in(),
            Key::Char('<') => match movement::climb(&self.scene.world, &mut self.scene.things, self.player, true) {
                Some(_) => self.z_offset = 0,
                None => self.say("There are no up stairs here."),
            },
            Key::Char('>') => match movement::climb(&self.scene.world, &mut self.scene.things, self.player, false) {
                Some(_) => self.z_offset = 0,
                None => self.say("There are no down stairs here."),
            },
            Key::Char('.') | Key::Char('5') => {}
            Key::PageUp => self.z_offset += 1,
            Key::PageDown => self.z_offset -= 1,
            Key::Char('C') => {
                let at = self.player_pos();
                let n = movement::close_doors_near(&mut self.scene.things, at);
                self.say(if n > 0 { "You close the door." } else { "No open door beside you." });
            }
            Key::Char('L') => {
                let (a, b) = (self.ids.you, self.ids.you_lit);
                let Some(t) = self.scene.things.get_mut(self.player) else { return Flow::Continue };
                if t.kind == b && t.lit {
                    t.kind = a;
                    self.say("You put out your lantern.");
                } else {
                    t.kind = b;
                    t.lit = false;
                    let msg = match flames::try_light(&self.scene.world, &mut self.scene.things, self.player, &mut self.rng) {
                        Lighting::Lit => "You light your lantern.",
                        Lighting::Sputters => "Your lantern is damp; the wick won't take.",
                        Lighting::Underwater => "Not underwater.",
                        _ => "",
                    };
                    self.say(msg);
                }
            }
            _ => return self.common_key(k),
        }
        Flow::Continue
    }

    fn on_key_up(&mut self, key: Key) {
        self.pad.release(key);
    }

    fn update(&mut self) {
        if let Some((dx, dy)) = self.pad.update(Instant::now()) {
            if let Some(cur) = self.aim {
                self.aim = Some(cur.offset(dx, dy, 0));
            } else if let Some(cur) = self.look {
                self.look = Some(cur.offset(dx, dy, 0));
            } else {
                self.walk(dx, dy);
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, stats: &LoopStats) {
        f.clear(Rgb::BLACK);
        let (fw, fh) = (f.width(), f.height());
        let panel_w = if fw >= 110 { 38 } else { 0 };
        let log_h = 2;
        let map = Rect::new(0, 0, fw - panel_w, fh - log_h);
        let p = self.player_pos();
        self.view.observer = if self.see_all { None } else { Some(p) };
        let cursor = self.aim.or(self.look);
        self.view.cursor = cursor;
        self.view.marks = match self.aim {
            Some(t) => tachyon::projectile::trace(&self.scene.world, &self.scene.things, Some(self.player), p, t),
            None => Vec::new(),
        };
        self.view.center = match cursor {
            Some(c) => {
                // Keep the cursor on screen without re-centring every step.
                let (cols, rows) = self.view.cells_for(map);
                let mut ctr = Pos::new(self.view.center.x, self.view.center.y, c.z);
                if (c.x - ctr.x).abs() > cols / 2 - 3 || (c.y - ctr.y).abs() > rows / 2 - 2 {
                    ctr.x = c.x;
                    ctr.y = c.y;
                }
                ctr
            }
            None => p.offset(0, 0, self.z_offset),
        };
        self.view.draw(&self.scene, f, map);
        if panel_w > 0 {
            self.draw_panel(f, Rect::new(fw - panel_w, 0, panel_w, fh - log_h), stats);
        }
        for (i, m) in self.log.iter().take(log_h as usize).enumerate() {
            let c = if i == 0 { Rgb(230, 230, 230) } else { Rgb(120, 120, 120) };
            f.text(1, fh - log_h + i as i32, m, c, Rgb::BLACK);
        }
    }

    fn ticks_per_second(&self) -> f64 {
        let asleep = self.scene.life.get(self.player).is_some_and(|c| c.mind.asleep);
        if self.paused {
            0.0
        } else if asleep {
            SPEEDS[SPEEDS.len() - 1]
        } else {
            SPEEDS[self.speed]
        }
    }
}

impl Sandbox {
    /// Throw a firebolt from where you stand at `target`, and let time run.
    fn throw_bolt(&mut self, target: Pos) {
        self.aim = None;
        self.pad.clear();
        self.paused = false;
        let from = self.player_pos();
        if target == from {
            self.say("You can't throw it at your own feet.");
            return;
        }
        let w = &self.scene.world;
        if w.index(from).is_some_and(|i| w.fluid[i] >= tachyon::projectile::DOUSE) {
            self.say("The flame dies in the water around you.");
            return;
        }
        let mut bolt = Projectile::launch(&mut self.scene.things, self.ids.firebolt, from, target, BOLT_SPEED, BOLT, Some(self.player));
        bolt.trail = Some(self.ids.smoke);
        self.scene.projectiles.push(bolt);
        self.say("You hurl a firebolt!");
    }

    /// A bank of fog over everything open to the sky near you.
    fn fog_rolls_in(&mut self) {
        let p = self.player_pos();
        let w = &mut self.scene.world;
        for y in p.y - 30..=p.y + 30 {
            for x in p.x - 40..=p.x + 40 {
                for z in [p.z, p.z - 1] {
                    let q = Pos::new(x, y, z);
                    if let Some(i) = w.index(q) {
                        if w.sky[i] > 0 && !w.tile[i].is_wall() && w.fluid[i] < 32 {
                            w.add_gas(q, self.ids.mist, 150);
                        }
                    }
                }
            }
        }
        self.say("Fog rolls in.");
    }

    /// Off the raft and into the water beside it.
    fn jump_overboard(&mut self) {
        let p = self.player_pos();
        let Some(v) = self.scene.vessel_at(p) else {
            self.say("You are not on a raft.");
            return;
        };
        let w = &self.scene.world;
        let deep = |q: Pos| w.index(q).is_some_and(|i| w.fluid[i] >= movement::SWIM_DEPTH);
        let spot = DIRS8.iter().map(|&(dx, dy)| p.offset(dx, dy, 0)).find_map(|q| {
            if self.scene.vessels[v].contains(q) || w.tile(q).is_wall() || self.scene.things.blocks_move(q) {
                None
            } else if deep(q) {
                Some(q)
            } else if deep(q.down()) && !w.tile(q).seals_below() {
                Some(q.down())
            } else {
                None
            }
        });
        match spot {
            Some(q) => {
                self.scene.things.move_to(self.player, q);
                self.say("Splash! You jump into the water.");
            }
            None => self.say("There is no deep water to jump into."),
        }
    }

    fn common_key(&mut self, k: KeyEvent) -> Flow {
        match k.key {
            Key::Char(' ') => self.paused = !self.paused,
            Key::Char('+') | Key::Char('=') => self.speed = (self.speed + 1).min(SPEEDS.len() - 1),
            Key::Char('-') => self.speed = self.speed.saturating_sub(1),
            Key::Char('v') => {
                self.see_all = !self.see_all;
                self.say(if self.see_all { "You see everything." } else { "You see what you can see." });
            }
            Key::Char('o') => self.view.overlay = self.view.overlay.next(),
            Key::Char('m') => self.view.wide = !self.view.wide,
            Key::Char('g') => self.view.lighting = !self.view.lighting,
            Key::Char('U') => {
                self.view.up_depth = if self.view.up_depth > 0 { 0 } else { 2 };
                self.say(if self.view.up_depth > 0 { "You see the hillsides above you." } else { "Higher ground shows as solid walls." });
            }
            Key::Char('T') => {
                self.scene.clock.skip(3600.0);
                self.say(format!("An hour passes. {}", self.scene.clock.label()));
            }
            Key::Char('?') => self.help = !self.help,
            _ => {}
        }
        Flow::Continue
    }
}

fn parse_size(s: &str) -> Option<(i32, i32, i32)> {
    let v: Vec<i32> = s.split('x').filter_map(|p| p.parse().ok()).collect();
    (v.len() == 3).then(|| (v[0], v[1], v[2]))
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (w, h, d) = arg("--size").and_then(|s| parse_size(&s)).unwrap_or((160, 100, 12));
    let seed: u64 = arg("--seed").and_then(|s| s.parse().ok()).unwrap_or(1805);

    let t = Instant::now();
    let mut game = Sandbox::new(w, h, d, seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1000.0;
    if let Some(r) = arg("--rain").and_then(|s| s.parse::<usize>().ok()) {
        game.rain = r.min(RAINS.len() - 1);
    }
    if let Some(wi) = arg("--wind").and_then(|s| s.parse::<usize>().ok()) {
        game.wind = wi.min(WINDS.len() - 1);
    }
    game.set_weather();

    if let Some(n) = arg("--bench").and_then(|s| s.parse::<u32>().ok()) {
        println!("generated {w}x{h}x{d} ({:.2}M cells) in {gen_ms:.0} ms", game.scene.world.len() as f64 / 1e6);
        if flag("--fire") {
            let (fx, fy) = (w / 2 + 18, h / 2 - 10);
            game.scene.world.ignite(Pos::new(fx, fy, G));
        }
        let t = Instant::now();
        let mut worst = 0u64;
        let mut awake = 0usize;
        let mut parts = [0u64; 4];
        for _ in 0..n {
            game.tick();
            let s = &game.scene.sim.stats;
            worst = worst.max(s.tick_us);
            awake += s.awake_chunks;
            for (p, us) in parts.iter_mut().zip([s.fluid_us, s.gas_us, s.heat_us, s.sky_us]) {
                *p += us;
            }
        }
        let el = t.elapsed().as_secs_f64();
        println!(
            "{n} ticks in {:.2} s: {:.3} ms/tick average, {:.2} ms worst, {:.0} awake chunks of {} on average",
            el,
            el * 1000.0 / n as f64,
            worst as f64 / 1000.0,
            awake as f64 / n as f64,
            game.scene.sim.stats.total_chunks
        );
        let ms = |us: u64| us as f64 / 1000.0 / n as f64;
        println!(
            "  per tick: fluid {:.3} ms, gas {:.3} ms, heat {:.3} ms, sky {:.3} ms",
            ms(parts[0]),
            ms(parts[1]),
            ms(parts[2]),
            ms(parts[3])
        );
        if flag("--fire") {
            let world = &game.scene.world;
            let ash = world.mats.id("ash");
            let burning = world.fire.iter().filter(|&&f| f > 0).count();
            let burnt = world.mat.iter().filter(|&&m| m == ash).count();
            println!("fire: {burning} cells burning, {burnt} burnt to ash");
        }
        let mut f = Frame::new(200, 60);
        let t = Instant::now();
        for _ in 0..100 {
            game.scene.clock.tick += 1; // force light recompute each frame
            game.draw(&mut f, &LoopStats::default());
        }
        println!("draw 200x60: {:.2} ms/frame (lighting recomputed every frame)", t.elapsed().as_secs_f64() * 10.0);
        return Ok(());
    }

    // Run the village for a while and print what the creatures did.
    if let Some(n) = arg("--watch").and_then(|s| s.parse::<u32>().ok()) {
        let t = Instant::now();
        let mut worst = 0.0f64;
        for _ in 0..n {
            let s = Instant::now();
            game.scene.tick();
            worst = worst.max(s.elapsed().as_secs_f64() * 1000.0);
            let life = &game.scene.life;
            for e in std::mem::take(&mut game.scene.life.events.clone()) {
                if !matches!(e, LifeEvent::Woke(_) | LifeEvent::Burned { .. }) {
                    println!("{}  {}", game.scene.clock.label(), life.narrate(&game.scene.things, &e));
                }
            }
            game.scene.life.events.clear();
        }
        println!("{n} ticks in {:.2} s, worst tick {worst:.1} ms", t.elapsed().as_secs_f64());
        let life = &game.scene.life;
        for c in &life.creatures {
            let lines = life.describe(&game.scene.things, c.thing);
            let p = game.scene.things.get(c.thing).map(|t| t.pos()).unwrap_or_default();
            println!("{} at ({},{},{}): {}", lines[0].0, p.x, p.y, p.z, lines[1..].iter().map(|l| l.0.as_str()).collect::<Vec<_>>().join(" | "));
        }
        return Ok(());
    }

    if let Some(path) = arg("--snapshot") {
        if let Some(hr) = arg("--hour").and_then(|s| s.parse::<f64>().ok()) {
            let now = game.scene.clock.hour();
            game.scene.clock.skip((hr - now).rem_euclid(24.0) * 3600.0);
        }
        let ticks: u32 = arg("--ticks").and_then(|s| s.parse().ok()).unwrap_or(20);
        if let Some(at) = arg("--at") {
            let v: Vec<i32> = at.split(',').filter_map(|p| p.parse().ok()).collect();
            game.scene.things.move_to(game.player, Pos::new(v[0], v[1], v[2]));
        }
        if flag("--fog") {
            game.fog_rolls_in();
        }
        // A knife cut to your thigh and a punch to the face, to see wounds and blood.
        if flag("--hurt") {
            let life = &mut game.scene.life;
            let plan = life.species[0].plan.clone();
            let mut rng = Rng::new(7);
            if let Some(c) = life.get_mut(game.player) {
                c.body.strike(&plan, plan.find("left thigh").unwrap(), DamageKind::Edge, 30.0, &mut rng);
                c.body.strike(&plan, plan.find("head").unwrap(), DamageKind::Blunt, 20.0, &mut rng);
            }
        }
        if let Some(t) = arg("--bolt") {
            let v: Vec<i32> = t.split(',').filter_map(|p| p.parse().ok()).collect();
            game.throw_bolt(Pos::new(v[0], v[1], v[2]));
        }
        if let Some(at) = arg("--fire-at") {
            let v: Vec<i32> = at.split(',').filter_map(|p| p.parse().ok()).collect();
            game.scene.world.ignite(Pos::new(v[0], v[1], v[2]));
        }
        for _ in 0..ticks {
            game.tick();
        }
        if let Some(z) = arg("--z").and_then(|s| s.parse::<i32>().ok()) {
            game.z_offset = z;
        }
        game.see_all = flag("--all");
        game.view.wide = !flag("--narrow");
        if flag("--noup") {
            game.view.up_depth = 0;
        }
        game.help = !flag("--nohelp");
        if flag("--lantern") {
            if let Some(t) = game.scene.things.get_mut(game.player) {
                t.kind = game.ids.you_lit;
            }
        }
        if let Some(o) = arg("--overlay") {
            while game.view.overlay.name() != o && game.view.overlay.next() != Overlay::None {
                game.view.overlay = game.view.overlay.next();
            }
        }
        let (fw, fh) = arg("--frame").and_then(|s| {
            let v: Vec<u16> = s.split('x').filter_map(|p| p.parse().ok()).collect();
            (v.len() == 2).then(|| (v[0], v[1]))
        }).unwrap_or((180, 54));
        let mut f = Frame::new(fw, fh);
        game.draw(&mut f, &LoopStats::default());
        game.draw(&mut f, &LoopStats::default()); // second pass: memory from the first
        std::fs::write(&path, f.to_html())?;
        std::fs::write(format!("{path}.txt"), f.to_text())?;
        println!("wrote {path}");
        return Ok(());
    }

    tachyon::app::run(&mut game, 60)
}
