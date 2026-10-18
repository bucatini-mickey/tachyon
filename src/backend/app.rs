//! The main loop. A game implements [`Game`]; [`run`] owns the terminal and
//! calls the game at a fixed simulation rate and a capped frame rate.
//!
//! Simulation and drawing are decoupled: the sim advances in whole ticks at
//! `ticks_per_second` (which the game can change, or set to 0 to pause), and
//! frames are drawn as fast as `max_fps` allows. Input is handled the moment
//! it arrives.

use crate::frame::Frame;
use crate::input::{Event, Key, KeyEvent, MouseEvent};
use crate::term::Terminal;
use std::io;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// Timing figures the loop measures, for the game to display.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoopStats {
    pub fps: f32,
    /// Time spent in `Game::draw`, ms (smoothed).
    pub draw_ms: f32,
    /// Time spent writing to the terminal, ms (smoothed).
    pub present_ms: f32,
    /// Time spent in `Game::tick` per tick, ms (smoothed).
    pub tick_ms: f32,
    /// Ticks actually run per second.
    pub tps: f32,
    pub bytes: usize,
    pub cells_changed: usize,
    /// Where key releases come from: "terminal", "keyboard poll", or "none yet".
    pub key_ups: &'static str,
}

pub trait Game {
    /// Advance the simulation one tick.
    fn tick(&mut self);
    /// Handle a key. Return `Flow::Quit` to end the program.
    fn on_key(&mut self, key: KeyEvent) -> Flow;
    /// A key was released (only on backends that report it).
    fn on_key_up(&mut self, _key: Key) {}
    /// A click, drag, move or scroll.
    fn on_mouse(&mut self, _m: MouseEvent) -> Flow {
        Flow::Continue
    }
    /// Called once per frame before drawing, for things that run on real
    /// time rather than simulation ticks (held-key walking, animation).
    fn update(&mut self) {}
    /// Draw into the frame (already sized to the terminal).
    fn draw(&mut self, frame: &mut Frame, stats: &LoopStats);
    /// Desired simulation rate; 0 pauses.
    fn ticks_per_second(&self) -> f64;
}

/// Run a game in the terminal until it quits (or Ctrl-C).
pub fn run<G: Game>(game: &mut G, max_fps: u32) -> io::Result<()> {
    let mut term = Terminal::enter()?;
    let (w, h) = Terminal::size();
    let mut frame = Frame::new(w, h);
    let mut stats = LoopStats::default();
    let frame_dt = Duration::from_secs_f64(1.0 / max_fps.max(1) as f64);
    let mut last = Instant::now();
    let mut acc = 0.0f64;
    let mut next_frame = Instant::now();
    let mut fps_window = (Instant::now(), 0u32, 0u32);
    let smooth = |old: f32, new: f32| if old == 0.0 { new } else { old * 0.9 + new * 0.1 };

    loop {
        // Input, until it's time for the next frame.
        loop {
            let now = Instant::now();
            let timeout = next_frame.saturating_duration_since(now);
            match term.poll(timeout)? {
                Some(Event::Key(k)) => {
                    if k.ctrl && k.key == Key::Char('c') {
                        return Ok(());
                    }
                    if k.ctrl && k.key == Key::Char('l') {
                        // Repaint everything (the classic terminal shortcut).
                        term.invalidate();
                        continue;
                    }
                    if game.on_key(k) == Flow::Quit {
                        return Ok(());
                    }
                }
                Some(Event::KeyUp(k)) => game.on_key_up(k),
                Some(Event::Mouse(m)) => {
                    if game.on_mouse(m) == Flow::Quit {
                        return Ok(());
                    }
                }
                Some(Event::Resize(w, h)) => {
                    frame.resize(w, h);
                    term.invalidate();
                }
                None => break,
            }
            if Instant::now() >= next_frame {
                break;
            }
        }
        next_frame = Instant::now() + frame_dt;

        // Simulation, at a fixed rate with bounded catch-up.
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64();
        last = now;
        let tps = game.ticks_per_second();
        if tps > 0.0 {
            acc += dt * tps;
            let mut n = 0;
            let t = Instant::now();
            while acc >= 1.0 && n < 16 {
                game.tick();
                acc -= 1.0;
                n += 1;
            }
            if n > 0 {
                stats.tick_ms = smooth(stats.tick_ms, t.elapsed().as_secs_f32() * 1000.0 / n as f32);
                fps_window.2 += n;
            }
            if n == 16 {
                acc = 0.0; // can't keep up: drop time rather than spiral
            }
        } else {
            acc = 0.0;
        }

        game.update();

        // Draw.
        let t = Instant::now();
        game.draw(&mut frame, &stats);
        stats.draw_ms = smooth(stats.draw_ms, t.elapsed().as_secs_f32() * 1000.0);
        let t = Instant::now();
        term.present(&frame)?;
        stats.present_ms = smooth(stats.present_ms, t.elapsed().as_secs_f32() * 1000.0);
        stats.bytes = term.last_bytes;
        stats.key_ups = term.key_ups();
        stats.cells_changed = term.last_cells;

        fps_window.1 += 1;
        let el = fps_window.0.elapsed().as_secs_f32();
        if el >= 0.5 {
            stats.fps = fps_window.1 as f32 / el;
            stats.tps = fps_window.2 as f32 / el;
            fps_window = (Instant::now(), 0, 0);
        }
    }
}
