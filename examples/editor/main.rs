//! Tachyon world editor: paint an atlas (land, height, rock, biomes, rivers,
//! features), then step into any part of it at 1 m scale to build
//! structures and save them as blueprints.
//!
//!     cargo run --release --example editor                  (edits world.atlas)
//!     cargo run --release --example editor -- isle.atlas    (or a named file)
//!
//! Blueprints live in ./blueprints as .tbp text files.

mod atlas_mode;
mod local_mode;
mod palette;

use std::collections::VecDeque;
use std::io;
use tachyon::atlas::Atlas;
use tachyon::frame::Cell;
use tachyon::prelude::*;

pub const BLUEPRINTS: &str = "blueprints";
pub const PANEL_W: i32 = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Atlas,
    Local,
}

/// What to do with the text typed into the prompt.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    FeatureLabel(usize),
    SaveBlueprint,
    StampBlueprint,
    Quit,
    LeaveLocal,
    NewAtlas,
}

pub struct Prompt {
    pub label: String,
    pub text: String,
    pub ask: Ask,
}

pub struct Editor {
    pub mode: Mode,
    pub atlas: Atlas,
    pub path: String,
    pub dirty: bool,
    pub undo: Vec<Atlas>,
    pub log: VecDeque<String>,
    pub prompt: Option<Prompt>,
    pub pad: DirPad,
    /// Mouse button held over the map, for painting strokes.
    pub painting: Option<MouseButton>,
    pub a: atlas_mode::AtlasState,
    pub local: Option<local_mode::Local>,
    pub seed: u32,
    /// Where the map was last drawn (for mouse clicks).
    pub map: Rect,
}

impl Editor {
    fn new(path: String) -> Editor {
        let (atlas, msg) = match std::fs::read_to_string(&path) {
            Ok(text) => match Atlas::from_text(&text) {
                Ok(a) => (a, format!("Opened {path}.")),
                Err(e) => (atlas_mode::rough_island(48, 32, 7), format!("Couldn't read {path} ({e}); started a new island.")),
            },
            Err(_) => (atlas_mode::rough_island(48, 32, 7), format!("New island; it will be saved to {path}.")),
        };
        let a = atlas_mode::AtlasState::new(&atlas);
        let mut e = Editor {
            mode: Mode::Atlas,
            atlas,
            path,
            dirty: false,
            undo: Vec::new(),
            log: VecDeque::new(),
            prompt: None,
            pad: DirPad::new(),
            painting: None,
            a,
            local: None,
            seed: 1805,
            map: Rect::new(0, 0, 0, 0),
        };
        e.say(msg);
        e.say("Paint with the mouse (left / right) or the keys. ? for help.");
        e
    }

    pub fn say(&mut self, m: impl Into<String>) {
        self.log.push_front(m.into());
        self.log.truncate(3);
    }

    pub fn ask(&mut self, label: &str, initial: &str, ask: Ask) {
        self.pad.clear();
        self.prompt = Some(Prompt { label: label.into(), text: initial.into(), ask });
    }

    /// Remember the atlas before a change, for undo.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.atlas.clone());
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.dirty = true;
    }

    pub fn save(&mut self) {
        match std::fs::write(&self.path, self.atlas.to_text()) {
            Ok(()) => {
                self.dirty = false;
                let p = self.path.clone();
                self.say(format!("Saved {p}."));
            }
            Err(e) => self.say(format!("Save failed: {e}")),
        }
    }

    fn answer(&mut self, ask: Ask, text: String) -> Flow {
        let yes = text.trim().eq_ignore_ascii_case("y") || text.trim().eq_ignore_ascii_case("yes");
        match ask {
            Ask::FeatureLabel(i) => {
                if let Some(f) = self.atlas.features.get_mut(i) {
                    f.label = text.trim().to_string();
                }
            }
            Ask::SaveBlueprint => self.local_save_blueprint(text.trim()),
            Ask::StampBlueprint => self.local_stamp_blueprint(text.trim()),
            Ask::Quit => {
                if yes {
                    return Flow::Quit;
                }
            }
            Ask::LeaveLocal => {
                if yes {
                    self.leave_local();
                }
            }
            Ask::NewAtlas => {
                if yes {
                    self.checkpoint();
                    self.atlas = Atlas::new("untitled", self.atlas.w, self.atlas.h, &palette::ROCKS, &palette::BIOMES.map(|b| b.name), 12);
                    self.say("A blank sea. Undo (u) brings the old map back.");
                }
            }
        }
        Flow::Continue
    }

    /// Names of the blueprints on disk.
    pub fn blueprint_names() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(BLUEPRINTS)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".tbp")).map(str::to_string))
            .collect();
        v.sort();
        v
    }

    fn draw_prompt(&self, f: &mut Frame) {
        let Some(p) = &self.prompt else { return };
        let (fw, fh) = (f.width(), f.height());
        let w = (fw - 10).clamp(30, 80);
        let r = Rect::new((fw - w) / 2, fh / 2 - 2, w, 4);
        f.fill(r, Cell::new(' ', Rgb::WHITE, Rgb(30, 30, 44)));
        f.border(r, Rgb(240, 200, 90), Rgb(30, 30, 44), "");
        f.text_clip(r.x + 2, r.y + 1, w - 4, &p.label, Rgb(240, 200, 90), Rgb(30, 30, 44));
        f.text_clip(r.x + 2, r.y + 2, w - 4, &format!("> {}_", p.text), Rgb::WHITE, Rgb(30, 30, 44));
    }
}

impl Game for Editor {
    fn tick(&mut self) {
        if let Some(l) = &mut self.local {
            if l.running {
                l.scene.tick();
            }
        }
    }

    fn ticks_per_second(&self) -> f64 {
        10.0
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        if let Some(p) = &mut self.prompt {
            match k.key {
                Key::Enter => {
                    let p = self.prompt.take().unwrap();
                    return self.answer(p.ask, p.text);
                }
                Key::Esc => self.prompt = None,
                Key::Backspace => {
                    p.text.pop();
                }
                Key::Char(c) if !k.ctrl && !k.alt => p.text.push(c),
                _ => {}
            }
            return Flow::Continue;
        }
        match self.mode {
            Mode::Atlas => self.atlas_key(k),
            Mode::Local => self.local_key(k),
        }
    }

    fn on_key_up(&mut self, key: Key) {
        self.pad.release(key);
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Flow {
        if self.prompt.is_some() {
            return Flow::Continue;
        }
        match self.mode {
            Mode::Atlas => self.atlas_mouse(m),
            Mode::Local => self.local_mouse(m),
        }
        Flow::Continue
    }

    fn update(&mut self) {
        if let Some((dx, dy)) = self.pad.update(std::time::Instant::now()) {
            match self.mode {
                Mode::Atlas => self.atlas_move(dx, dy),
                Mode::Local => self.local_move(dx, dy),
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, stats: &LoopStats) {
        f.clear(Rgb::BLACK);
        let (fw, fh) = (f.width(), f.height());
        let panel = if fw >= 100 { PANEL_W } else { 0 };
        let map = Rect::new(0, 0, fw - panel, fh - 2);
        let side = Rect::new(fw - panel, 0, panel, fh - 2);
        self.map = map;
        match self.mode {
            Mode::Atlas => self.atlas_draw(f, map, side),
            Mode::Local => self.local_draw(f, map, side, stats),
        }
        for (i, m) in self.log.iter().take(2).enumerate() {
            let c = if i == 0 { Rgb(230, 230, 230) } else { Rgb(120, 120, 120) };
            f.text_clip(1, fh - 2 + i as i32, fw - 2, m, c, Rgb::BLACK);
        }
        self.draw_prompt(f);
    }
}

/// Panel text helper: writes a line and moves down.
pub struct Panel {
    pub r: Rect,
    pub y: i32,
}

pub const BG: Rgb = Rgb(12, 12, 16);
pub const TITLE: Rgb = Rgb(240, 200, 90);
pub const TXT: Rgb = Rgb(200, 200, 205);
pub const DIM: Rgb = Rgb(120, 120, 130);

impl Panel {
    pub fn new(f: &mut Frame, r: Rect) -> Panel {
        f.fill(r, Cell::new(' ', Rgb::WHITE, BG));
        Panel { r, y: r.y + 1 }
    }
    pub fn line(&mut self, f: &mut Frame, s: &str, c: Rgb) {
        if self.y < self.r.bottom() - 1 && self.r.w > 3 {
            f.text_clip(self.r.x + 2, self.y, self.r.w - 3, s, c, BG);
        }
        self.y += 1;
    }
    /// A line with a colour swatch first.
    pub fn swatch(&mut self, f: &mut Frame, cell: Cell, s: &str, c: Rgb) {
        if self.y < self.r.bottom() - 1 && self.r.w > 6 {
            f.set(self.r.x + 2, self.y, cell);
            f.set(self.r.x + 3, self.y, Cell::new(' ', cell.fg, cell.bg));
            f.text_clip(self.r.x + 5, self.y, self.r.w - 6, s, c, BG);
        }
        self.y += 1;
    }
    pub fn gap(&mut self) {
        self.y += 1;
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let path = args.iter().find(|a| !a.starts_with("--") && Some(*a) != arg("--snapshot").as_ref() && Some(*a) != arg("--local").as_ref() && Some(*a) != arg("--view").as_ref() && Some(*a) != arg("--ticks").as_ref()).cloned().unwrap_or_else(|| "world.atlas".to_string());
    let _ = std::fs::create_dir_all(BLUEPRINTS);
    let mut ed = Editor::new(path);

    // Headless: draw one screen to an HTML file (and .txt) and stop.
    //     editor -- --snapshot out.html [--view 0-3] [--local x,y] [--ticks n]
    if let Some(out) = arg("--snapshot") {
        if let Some(v) = arg("--view").and_then(|s| s.parse::<usize>().ok()) {
            for _ in 0..v {
                ed.atlas_key(KeyEvent::plain(Key::Tab));
            }
        }
        if let Some(c) = arg("--local") {
            let v: Vec<i32> = c.split(',').filter_map(|s| s.parse().ok()).collect();
            ed.enter_local(v[0], v[1]);
            if let Some(n) = arg("--ticks").and_then(|s| s.parse::<u32>().ok()) {
                if let Some(l) = &mut ed.local {
                    for _ in 0..n {
                        l.scene.tick();
                    }
                }
            }
        }
        let mut f = Frame::new(170, 50);
        ed.draw(&mut f, &LoopStats::default());
        // A key script, e.g. --keys "Enter 3 Space Right*6 Down*4 Space": names
        // (Enter Esc Tab Space Bksp Up Down Left Right PgUp PgDn), single
        // characters, "word:text" to type text, and "*n" to repeat.
        if let Some(script) = arg("--keys") {
            for tok in script.split_whitespace() {
                let (name, n) = match tok.rsplit_once('*') {
                    Some((a, b)) if b.parse::<u32>().is_ok() => (a, b.parse().unwrap()),
                    _ => (tok, 1),
                };
                for _ in 0..n {
                    if let Some(text) = name.strip_prefix("word:") {
                        for c in text.chars() {
                            ed.on_key(KeyEvent::plain(Key::Char(c)));
                        }
                        continue;
                    }
                    let dir = match name {
                        "Up" => Some((0, -1)),
                        "Down" => Some((0, 1)),
                        "Left" => Some((-1, 0)),
                        "Right" => Some((1, 0)),
                        _ => None,
                    };
                    if let Some((dx, dy)) = dir {
                        match ed.mode {
                            Mode::Atlas => ed.atlas_move(dx, dy),
                            Mode::Local => ed.local_move(dx, dy),
                        }
                        continue;
                    }
                    let key = match name {
                        "Enter" => Key::Enter,
                        "Esc" => Key::Esc,
                        "Tab" => Key::Tab,
                        "Space" => Key::Char(' '),
                        "Bksp" => Key::Backspace,
                        "PgUp" => Key::PageUp,
                        "PgDn" => Key::PageDown,
                        s => Key::Char(s.chars().next().unwrap_or(' ')),
                    };
                    ed.on_key(KeyEvent::plain(key));
                    ed.draw(&mut f, &LoopStats::default());
                }
            }
            ed.draw(&mut f, &LoopStats::default());
        }
        std::fs::write(&out, f.to_html())?;
        std::fs::write(format!("{out}.txt"), f.to_text())?;
        println!("wrote {out}");
        return Ok(());
    }
    tachyon::app::run(&mut ed, 60)
}
