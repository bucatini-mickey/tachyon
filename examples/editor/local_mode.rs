//! Local mode: the atlas area generated at 1 m, for building structures and
//! saving them as blueprints.

use crate::palette;
use crate::{Ask, Editor, Mode, Panel, BLUEPRINTS, DIM, TITLE, TXT};
use tachyon::atlas::{Feature, REGION};
use tachyon::blueprint::Blueprint;
use tachyon::frame::Cell;
use tachyon::material::MatKind;
use tachyon::prelude::*;
use tachyon::worldgen;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Paint,
    Rect,
    Room,
    Thing,
    Water,
    Select,
}

const TOOLS: [(Tool, &str); 6] =
    [(Tool::Paint, "paint"), (Tool::Rect, "fill rectangle"), (Tool::Room, "room"), (Tool::Thing, "furniture"), (Tool::Water, "water"), (Tool::Select, "select")];

const TILES: [(Tile, &str); 7] = [
    (Tile::Wall, "wall"),
    (Tile::Floor, "floor"),
    (Tile::Open, "empty (erase)"),
    (Tile::Ramp, "ramp"),
    (Tile::StairUp, "stair up"),
    (Tile::StairDown, "stair down"),
    (Tile::StairUpDown, "stair up/down"),
];

pub struct Local {
    pub scene: Scene,
    pub gen_origin: (i32, i32),
    pub sea_z: i32,
    pub view: MapView,
    pub cursor: Pos,
    pub tool: Tool,
    pub tile: usize,
    pub mat: usize,
    /// Solid materials you can build with.
    pub mats: Vec<MatId>,
    pub kind: usize,
    pub kinds: Vec<KindId>,
    /// First corner of a rectangle, room or selection being drawn.
    pub anchor: Option<Pos>,
    /// Selected box: corners on the cursor's level, and how many levels up.
    pub sel: Option<(Pos, Pos)>,
    pub sel_levels: i32,
    pub running: bool,
    /// Edits since the last blueprint save.
    pub edited: bool,
    pub undo: Vec<(World, Things)>,
    last_cell: Option<Pos>,
}

impl Local {
    fn snapshot(&mut self) {
        self.undo.push((self.scene.world.clone(), self.scene.things.clone()));
        if self.undo.len() > 8 {
            self.undo.remove(0);
        }
        self.edited = true;
    }

    fn sel_box(&self) -> Option<(Pos, i32, i32, i32)> {
        let (a, b) = self.sel?;
        let lo = Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z));
        Some((lo, (a.x - b.x).abs() + 1, (a.y - b.y).abs() + 1, self.sel_levels))
    }
}

/// Cells on the outline of a rectangle (for previews).
fn outline(a: Pos, b: Pos) -> Vec<Pos> {
    let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
    let mut v = Vec::new();
    for x in x0..=x1 {
        v.push(Pos::new(x, y0, b.z));
        v.push(Pos::new(x, y1, b.z));
    }
    for y in y0..=y1 {
        v.push(Pos::new(x0, y, b.z));
        v.push(Pos::new(x1, y, b.z));
    }
    v
}

impl Editor {
    /// Generate the 3x3 atlas cells around (cx, cy) and step in.
    pub fn enter_local(&mut self, cx: i32, cy: i32) {
        let (aw, ah) = (3.min(self.atlas.w), 3.min(self.atlas.h));
        let ax0 = (cx - 1).clamp(0, self.atlas.w - aw);
        let ay0 = (cy - 1).clamp(0, self.atlas.h - ah);
        let set = palette::gen_settings(self.seed);
        let t = std::time::Instant::now();
        let g = worldgen::generate(&self.atlas, &set, ax0, ay0, aw, ah);
        let ms = t.elapsed().as_millis();
        let mut things = Things::new();
        palette::register_kinds(&mut things);
        let kinds: Vec<KindId> = things.kinds().map(|(id, _)| id).collect();
        let mats: Vec<MatId> = (0..g.world.mats.len() as MatId)
            .filter(|&m| m != AIR && g.world.mats.get(m).kind == MatKind::Solid)
            .collect();
        let mut scene = Scene::new(g.world, things, Clock::new(2.0, 12.0), self.seed as u64);
        scene.sim.sources = g.sources.clone();
        // Stamp the blueprints placed on the atlas here.
        let mut stamped = 0;
        let mut missing = Vec::new();
        for (f, p) in &g.features {
            let Some(name) = f.blueprint() else { continue };
            match std::fs::read_to_string(format!("{BLUEPRINTS}/{name}.tbp")).map_err(|e| e.to_string()).and_then(|t| Blueprint::from_text(&t)) {
                Ok(bp) => {
                    bp.stamp(&mut scene.world, &mut scene.things, *p);
                    stamped += 1;
                }
                Err(_) => missing.push(name.to_string()),
            }
        }
        tachyon::sim::light::compute_sky(&mut scene.world, &mut std::collections::VecDeque::new());
        // Start on the ground at the centre of the area.
        let (mx, my) = ((cx - ax0) * REGION + REGION / 2, (cy - ay0) * REGION + REGION / 2);
        let w = &scene.world;
        let mz = (0..w.d).rev().find(|&z| w.standable(Pos::new(mx, my, z)) || w.fluid[w.idx(mx, my, z)] > 0).unwrap_or(g.sea_z);
        let mut view = MapView::new();
        view.wide = true;
        view.observer = None;
        view.center = Pos::new(mx, my, mz);
        let sel_mat = mats.iter().position(|&m| scene.world.mats.get(m).name == "stone block").unwrap_or(0);
        self.local = Some(Local {
            scene,
            gen_origin: g.origin,
            sea_z: g.sea_z,
            view,
            cursor: Pos::new(mx, my, mz),
            tool: Tool::Paint,
            tile: 0,
            mat: sel_mat,
            mats,
            kind: 0,
            kinds,
            anchor: None,
            sel: None,
            sel_levels: 3,
            running: false,
            edited: false,
            undo: Vec::new(),
            last_cell: None,
        });
        self.mode = Mode::Local;
        self.pad.clear();
        self.say(format!(
            "Generated {}x{} m in {ms} ms{}. Esc returns to the atlas; P runs the physics.",
            aw * REGION,
            ah * REGION,
            if stamped > 0 { format!(", {stamped} blueprint(s) placed") } else { String::new() }
        ));
        if !missing.is_empty() {
            self.say(format!("Missing blueprints: {}", missing.join(", ")));
        }
    }

    pub fn leave_local(&mut self) {
        self.local = None;
        self.mode = Mode::Atlas;
        self.pad.clear();
        self.say("Back on the atlas.");
    }

    pub fn local_move(&mut self, dx: i32, dy: i32) {
        if let Some(l) = &mut self.local {
            let w = &l.scene.world;
            l.cursor = Pos::new((l.cursor.x + dx).clamp(0, w.w - 1), (l.cursor.y + dy).clamp(0, w.h - 1), l.cursor.z);
        }
    }

    /// The tool's main action at a cell (`primary`), or its other one.
    fn local_apply(&mut self, p: Pos, primary: bool) {
        let Some(l) = &mut self.local else { return };
        let mut msg: Option<String> = None;
        let (tile, _) = TILES[l.tile];
        let mat = if tile == Tile::Open { AIR } else { l.mats[l.mat] };
        match l.tool {
            Tool::Paint => {
                if primary {
                    l.scene.world.set_tile(p, tile, mat);
                } else if let Some(i) = l.scene.world.index(p) {
                    // Pick up what is there.
                    let t = l.scene.world.tile[i];
                    if let Some(k) = TILES.iter().position(|(x, _)| *x == t) {
                        l.tile = k;
                    }
                    if let Some(k) = l.mats.iter().position(|&m| m == l.scene.world.mat[i]) {
                        l.mat = k;
                    }
                }
            }
            Tool::Rect | Tool::Room | Tool::Select => {
                if !primary {
                    l.anchor = None;
                    if l.tool == Tool::Select {
                        l.sel = None;
                    }
                } else if let Some(a) = l.anchor.take() {
                    let b = Pos::new(p.x, p.y, a.z);
                    let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
                    match l.tool {
                        Tool::Select => {
                            l.sel = Some((a, b));
                            msg = Some(format!("Selected {}x{} x {} levels. S saves it as a blueprint.", x1 - x0 + 1, y1 - y0 + 1, l.sel_levels));
                        }
                        Tool::Rect => {
                            for y in y0..=y1 {
                                for x in x0..=x1 {
                                    l.scene.world.set_tile(Pos::new(x, y, a.z), tile, mat);
                                }
                            }
                        }
                        _ => {
                            // A room: walls round the edge, floor inside, of the chosen material.
                            let m = l.mats[l.mat];
                            for y in y0..=y1 {
                                for x in x0..=x1 {
                                    let edge = x == x0 || x == x1 || y == y0 || y == y1;
                                    l.scene.world.set_tile(Pos::new(x, y, a.z), if edge { Tile::Wall } else { Tile::Floor }, m);
                                }
                            }
                        }
                    }
                } else {
                    l.anchor = Some(p);
                    msg = Some("First corner set; now the opposite corner.".into());
                }
            }
            Tool::Thing => {
                if primary {
                    l.scene.things.spawn(l.kinds[l.kind], p);
                } else {
                    let ids: Vec<_> = l.scene.things.at(p).map(|(id, _)| id).collect();
                    for id in ids {
                        l.scene.things.despawn(id);
                    }
                }
            }
            Tool::Water => {
                let water = l.scene.world.mats.find("water").unwrap();
                if primary {
                    l.scene.world.add_fluid(p, water, FULL);
                } else {
                    l.scene.world.set_fluid(p, water, 0);
                }
            }
        }
        if let Some(m) = msg {
            self.say(m);
        }
    }

    pub fn local_key(&mut self, k: KeyEvent) -> Flow {
        let now = std::time::Instant::now();
        let dir = match k.key {
            Key::Up => Some((0, -1)),
            Key::Down => Some((0, 1)),
            Key::Left => Some((-1, 0)),
            Key::Right => Some((1, 0)),
            Key::Home => Some((-1, -1)),
            Key::End => Some((-1, 1)),
            _ => None,
        };
        if let Some(d) = dir {
            self.pad.press(k.key, d, now);
            return Flow::Continue;
        }
        if k.ctrl && k.key == Key::Char('z') {
            self.local_undo();
            return Flow::Continue;
        }
        if k.ctrl && k.key == Key::Char('s') {
            self.save();
            return Flow::Continue;
        }
        let Some(l) = &mut self.local else { return Flow::Continue };
        match k.key {
            Key::Char(c @ '1'..='6') => {
                l.tool = TOOLS[(c as u8 - b'1') as usize].0;
                l.anchor = None;
                let name = TOOLS[(c as u8 - b'1') as usize].1;
                self.say(format!("Tool: {name}."));
            }
            Key::Char('<') | Key::PageUp => l.cursor.z = (l.cursor.z + 1).min(l.scene.world.d - 1),
            Key::Char('>') | Key::PageDown => l.cursor.z = (l.cursor.z - 1).max(0),
            Key::Char('t') => l.tile = (l.tile + 1) % TILES.len(),
            Key::Char('T') => l.tile = (l.tile + TILES.len() - 1) % TILES.len(),
            Key::Char(']') => l.mat = (l.mat + 1) % l.mats.len(),
            Key::Char('[') => l.mat = (l.mat + l.mats.len() - 1) % l.mats.len(),
            Key::Char('.') => l.kind = (l.kind + 1) % l.kinds.len(),
            Key::Char(',') => l.kind = (l.kind + l.kinds.len() - 1) % l.kinds.len(),
            Key::Char('+') | Key::Char('=') => l.sel_levels = (l.sel_levels + 1).min(40),
            Key::Char('-') => l.sel_levels = (l.sel_levels - 1).max(1),
            Key::Char('P') => {
                l.running = !l.running;
                let m = if l.running { "Physics running: water flows, fire burns." } else { "Physics paused." };
                self.say(m);
            }
            Key::Char(' ') => {
                let p = l.cursor;
                l.snapshot();
                self.local_apply(p, true);
            }
            Key::Backspace | Key::Delete => {
                let p = l.cursor;
                l.snapshot();
                self.local_apply(p, false);
            }
            Key::Char('u') => self.local_undo(),
            Key::Char('S') => {
                if l.sel.is_none() {
                    self.say("Select an area first (tool 6: two corners).");
                } else {
                    self.ask("Save the selection as blueprint named:", "", Ask::SaveBlueprint);
                }
            }
            Key::Char('B') => {
                let names = Editor::blueprint_names();
                let hint = if names.is_empty() { "none saved yet".to_string() } else { names.join(", ") };
                self.ask(&format!("Stamp which blueprint at the cursor? ({hint})"), "", Ask::StampBlueprint);
            }
            Key::Char('?') => self.a.help = !self.a.help,
            Key::Esc => {
                if l.anchor.is_some() {
                    l.anchor = None;
                } else if l.edited {
                    self.ask("Changes not saved as a blueprint are lost. Leave? (y / n)", "", Ask::LeaveLocal);
                } else {
                    self.leave_local();
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn local_undo(&mut self) {
        let Some(l) = &mut self.local else { return };
        match l.undo.pop() {
            Some((w, t)) => {
                l.scene.world = w;
                l.scene.things = t;
                self.say("Undone.");
            }
            None => self.say("Nothing to undo."),
        }
    }

    fn local_cell(&self, sx: i32, sy: i32) -> Option<Pos> {
        let l = self.local.as_ref()?;
        let map = self.map;
        if !map.contains(sx, sy) {
            return None;
        }
        let (cols, rows) = l.view.cells_for(map);
        let (x0, y0) = (l.view.center.x - cols / 2, l.view.center.y - rows / 2);
        let x = x0 + if l.view.wide { (sx - map.x) / 2 } else { sx - map.x };
        let p = Pos::new(x, y0 + (sy - map.y), l.cursor.z);
        l.scene.world.index(p).map(|_| p)
    }

    pub fn local_mouse(&mut self, m: MouseEvent) {
        match m.kind {
            MouseKind::Down(b) | MouseKind::Drag(b) => {
                let Some(p) = self.local_cell(m.x, m.y) else { return };
                let fresh = matches!(m.kind, MouseKind::Down(_));
                let Some(l) = &mut self.local else { return };
                l.cursor = p;
                if b == MouseButton::Middle {
                    return;
                }
                // Corner tools and furniture act per click; paint and water smear.
                let once = matches!(l.tool, Tool::Rect | Tool::Room | Tool::Select | Tool::Thing);
                if fresh {
                    l.snapshot();
                } else if once || l.last_cell == Some(p) {
                    return;
                }
                l.last_cell = Some(p);
                self.local_apply(p, b == MouseButton::Left);
            }
            MouseKind::Up(_) => {
                if let Some(l) = &mut self.local {
                    l.last_cell = None;
                }
            }
            MouseKind::Move => {
                if let Some(p) = self.local_cell(m.x, m.y) {
                    if let Some(l) = &mut self.local {
                        l.cursor = p;
                    }
                }
            }
            MouseKind::ScrollUp => {
                if let Some(l) = &mut self.local {
                    l.cursor.z = (l.cursor.z + 1).min(l.scene.world.d - 1);
                }
            }
            MouseKind::ScrollDown => {
                if let Some(l) = &mut self.local {
                    l.cursor.z = (l.cursor.z - 1).max(0);
                }
            }
        }
    }

    pub fn local_save_blueprint(&mut self, name: &str) {
        let name: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
        if name.is_empty() {
            self.say("A blueprint needs a name.");
            return;
        }
        let Some(l) = &mut self.local else { return };
        let Some((lo, w, h, d)) = l.sel_box() else { return };
        let bp = Blueprint::capture(&name, &l.scene.world, &l.scene.things, lo, w, h, d);
        let path = format!("{BLUEPRINTS}/{name}.tbp");
        if let Err(e) = std::fs::write(&path, bp.to_text()) {
            self.say(format!("Couldn't save {path}: {e}"));
            return;
        }
        l.edited = false;
        // Record it on the atlas, so it is built here every time.
        let (gx, gy, gz) = (lo.x + l.gen_origin.0, lo.y + l.gen_origin.1, lo.z - l.sea_z);
        let kind = format!("bp:{name}");
        let exists = self.atlas.features.iter().any(|f| f.kind == kind && f.x == gx && f.y == gy && f.z == gz);
        if !exists {
            self.checkpoint();
            self.atlas.features.push(Feature { x: gx, y: gy, z: gz, kind, label: name.clone() });
        }
        self.say(format!("Saved {path} ({w}x{h}x{d}) and placed it on the atlas. Save the atlas (S on the map) to keep it."));
    }

    pub fn local_stamp_blueprint(&mut self, name: &str) {
        let path = format!("{BLUEPRINTS}/{name}.tbp");
        let bp = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Blueprint::from_text(&t)) {
            Ok(bp) => bp,
            Err(e) => {
                self.say(format!("Couldn't load {path}: {e}"));
                return;
            }
        };
        let Some(l) = &mut self.local else { return };
        l.snapshot();
        let at = l.cursor;
        let r = bp.stamp(&mut l.scene.world, &mut l.scene.things, at);
        let (gx, gy, gz) = (at.x + l.gen_origin.0, at.y + l.gen_origin.1, at.z - l.sea_z);
        self.checkpoint();
        self.atlas.features.push(Feature { x: gx, y: gy, z: gz, kind: format!("bp:{name}"), label: name.to_string() });
        let mut m = format!("Stamped {name} ({} cells) and placed it on the atlas.", r.cells);
        if !r.unknown_materials.is_empty() || !r.unknown_kinds.is_empty() {
            m += &format!(" Unknown: {}", [r.unknown_materials, r.unknown_kinds].concat().join(", "));
        }
        self.say(m);
    }

    pub fn local_draw(&mut self, f: &mut Frame, map: Rect, side: Rect, stats: &LoopStats) {
        let Some(l) = &mut self.local else { return };
        // Camera: follow the cursor without re-centring on every step.
        let (cols, rows) = l.view.cells_for(map);
        let c = l.cursor;
        let mut ctr = Pos::new(l.view.center.x, l.view.center.y, c.z);
        if (c.x - ctr.x).abs() > cols / 2 - 3 || (c.y - ctr.y).abs() > rows / 2 - 2 {
            ctr.x = c.x;
            ctr.y = c.y;
        }
        l.view.center = ctr;
        l.view.cursor = Some(c);
        l.view.marks = match (l.anchor, l.sel) {
            (Some(a), _) => outline(Pos::new(a.x, a.y, c.z), c),
            (None, Some((a, b))) if a.z == c.z || (c.z >= a.z && c.z < a.z + l.sel_levels) => {
                outline(Pos::new(a.x, a.y, c.z), Pos::new(b.x, b.y, c.z))
            }
            _ => Vec::new(),
        };
        l.view.draw(&l.scene, f, map);

        if side.w <= 0 {
            return;
        }
        let mut p = Panel::new(f, side);
        p.line(f, "TACHYON  building", TITLE);
        let (gx, gy, gz) = (c.x + l.gen_origin.0, c.y + l.gen_origin.1, c.z - l.sea_z);
        p.line(f, &format!("at {gx}, {gy} m   {gz:+} m (level {})", c.z), TXT);
        p.line(f, &format!("physics: {}", if l.running { "running" } else { "paused (P)" }), TXT);
        p.line(f, &format!("{:.0} fps  draw {:.1} ms", stats.fps, stats.draw_ms), DIM);
        p.gap();
        let tool = TOOLS.iter().find(|(t, _)| *t == l.tool).map(|(_, n)| *n).unwrap_or("");
        p.line(f, &format!("TOOL  {}", tool.to_uppercase()), TITLE);
        let m = l.scene.world.mats.get(l.mats[l.mat]);
        match l.tool {
            Tool::Paint | Tool::Rect => {
                p.line(f, &format!("shape: {}  (t)", TILES[l.tile].1), TXT);
                p.swatch(f, Cell::new(m.wall_glyph, m.fg, m.bg), &format!("{}  ([ ])", m.name), TXT);
            }
            Tool::Room => p.swatch(f, Cell::new(m.wall_glyph, m.fg, m.bg), &format!("walls + floor: {}  ([ ])", m.name), TXT),
            Tool::Thing => {
                let k = l.scene.things.kind(l.kinds[l.kind]);
                p.swatch(f, Cell::new(k.glyph, k.fg, Rgb(30, 30, 36)), &format!("{}  (, .)", k.name), TXT);
            }
            Tool::Water => p.line(f, "pour a full cell; right: drain", TXT),
            Tool::Select => {
                match l.sel_box() {
                    Some((_, w, h, d)) => p.line(f, &format!("{w} x {h} x {d} levels  (+ -)"), TXT),
                    None => p.line(f, &format!("levels: {}  (+ -)", l.sel_levels), TXT),
                }
                p.line(f, "S save blueprint   B stamp one", DIM);
            }
        }
        if l.anchor.is_some() {
            p.line(f, "click the opposite corner", Rgb(255, 220, 90));
        }
        p.gap();
        p.line(f, "HERE", TITLE);
        for s in tachyon::view::inspect::describe(&l.scene, c).into_iter().skip(1).take(5) {
            p.line(f, &s, TXT);
        }
        p.gap();
        if self.a.help {
            for (i, s) in [
                "KEYS",
                "1 paint 2 rect 3 room 4 furniture",
                "5 water 6 select",
                "Space / left    use tool",
                "Bksp / right    pick / remove",
                "< > / wheel     level up / down",
                "t shape  [ ] material  , . item",
                "S save selection   B stamp",
                "P physics   u undo   Esc atlas",
            ]
            .iter()
            .enumerate()
            {
                p.line(f, s, if i == 0 { TITLE } else { DIM });
            }
        }
    }
}
