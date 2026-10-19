//! The terminal backend: raw mode, input, and a diffing renderer.
//!
//! `present` compares the new frame with the last one shown and writes only
//! the cells that changed, skipping cursor moves for runs of adjacent ASCII
//! cells and colour codes when the colour is unchanged. Every cell goes out
//! through the terminal's [`Display`]: pure ASCII by default, in the chosen
//! colour scheme. In `Extended` mode a non-ASCII glyph is always followed by
//! an explicit cursor move: terminals disagree about how wide some of them
//! are, and trusting the advance lets a row drift. The whole update goes out
//! in a single write wrapped in "synchronized output" markers, so terminals
//! that support them (Windows Terminal, kitty, WezTerm, iTerm2) never show a
//! half-drawn frame.

use crate::display::{push_num, Display, Ink};
use crate::frame::Frame;
use crate::input::{Event, Key, KeyEvent, MouseButton, MouseEvent, MouseKind};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers, MouseButton as CButton, MouseEventKind};
use crossterm::{cursor, execute, terminal};
use std::io::{self, Write};
use std::time::Duration;

pub struct Terminal {
    /// Keys pressed and not yet released, for synthesising key-ups.
    #[cfg_attr(not(windows), allow(dead_code))]
    down: Vec<Key>,
    real_releases: bool,
    polled_releases: bool,
    out: io::Stdout,
    prev: Option<Frame>,
    buf: Vec<u8>,
    /// How cells become characters and colours (see `set_display`).
    display: Display,
    /// Bytes written by the last `present`.
    pub last_bytes: usize,
    /// Cells changed by the last `present`.
    pub last_cells: usize,
}

/// Ask for key-release events where the terminal can send them (the kitty
/// keyboard protocol). The Windows console always reports them.
#[cfg(not(windows))]
fn enhance_keys(out: &mut io::Stdout) {
    use crossterm::event::{KeyboardEnhancementFlags as F, PushKeyboardEnhancementFlags};
    if terminal::supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(out, PushKeyboardEnhancementFlags(F::DISAMBIGUATE_ESCAPE_CODES | F::REPORT_EVENT_TYPES));
    }
}

#[cfg(windows)]
fn enhance_keys(_out: &mut io::Stdout) {}

fn restore() {
    let mut out = io::stdout();
    let _ = execute!(out, event::DisableMouseCapture);
    #[cfg(not(windows))]
    if terminal::supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(out, crossterm::event::PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, terminal::EnableLineWrap, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}

impl Terminal {
    /// Take over the terminal: alternate screen, raw input, hidden cursor.
    /// The terminal is restored when this is dropped, or if the program panics.
    pub fn enter() -> io::Result<Terminal> {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            hook(info);
        }));
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            terminal::EnterAlternateScreen,
            terminal::DisableLineWrap,
            cursor::Hide,
            terminal::Clear(terminal::ClearType::All)
        )?;
        enhance_keys(&mut out);
        execute!(out, event::EnableMouseCapture)?;
        Ok(Terminal { down: Vec::new(), real_releases: false, polled_releases: false, out, prev: None, buf: Vec::with_capacity(1 << 16), display: Display::from_env(), last_bytes: 0, last_cells: 0 })
    }

    pub fn size() -> (u16, u16) {
        terminal::size().unwrap_or((80, 24))
    }

    pub fn display(&self) -> Display {
        self.display
    }

    /// Change how cells reach the screen; repaints everything if it changed.
    pub fn set_display(&mut self, d: Display) {
        if d != self.display {
            self.display = d;
            self.invalidate();
        }
    }

    /// Force the next `present` to redraw everything.
    pub fn invalidate(&mut self) {
        self.prev = None;
    }

    pub fn present(&mut self, f: &Frame) -> io::Result<()> {
        let full = match &self.prev {
            Some(p) => p.width() != f.width() || p.height() != f.height(),
            None => true,
        };
        let buf = &mut self.buf;
        buf.clear();
        buf.extend_from_slice(b"\x1b[?2026h");
        if full {
            buf.extend_from_slice(b"\x1b[0m\x1b[2J");
        }
        let (w, h) = (f.width(), f.height());
        let cells = f.cells();
        let prev = self.prev.as_ref().map(|p| p.cells());
        let d = self.display;
        let mut fg: Option<Ink> = None;
        let mut bg: Option<Ink> = None;
        let mut cursor: Option<(i32, i32)> = None;
        let mut changed = 0;
        let mut utf8 = [0u8; 4];
        for y in 0..h {
            // After a non-ASCII glyph the next cell is always redrawn: if the
            // terminal drew that glyph two columns wide, it spilled into it.
            let mut force = false;
            for x in 0..w {
                let i = (y * w + x) as usize;
                let c = cells[i];
                let forced = std::mem::take(&mut force);
                if !full && !forced {
                    if let Some(p) = prev {
                        if p[i] == c {
                            continue;
                        }
                    }
                }
                changed += 1;
                if cursor != Some((x, y)) {
                    buf.extend_from_slice(b"\x1b[");
                    push_num(buf, (y + 1) as u32);
                    buf.push(b';');
                    push_num(buf, (x + 1) as u32);
                    buf.push(b'H');
                }
                let (f_ink, b_ink) = (d.ink(c.fg, false), d.ink(c.bg, true));
                if fg != Some(f_ink) {
                    Display::push_sgr(buf, f_ink, false);
                    fg = Some(f_ink);
                }
                if bg != Some(b_ink) {
                    Display::push_sgr(buf, b_ink, true);
                    bg = Some(b_ink);
                }
                let ch = d.glyph(c.ch);
                buf.extend_from_slice(ch.encode_utf8(&mut utf8).as_bytes());
                // How far the cursor moves after a non-ASCII glyph depends on
                // the terminal and font (some draw box, block and symbol
                // characters two columns wide), so only trust ASCII to
                // advance it by one; otherwise move explicitly next time.
                if ch.is_ascii() {
                    cursor = Some((x + 1, y));
                } else {
                    cursor = None;
                    force = true;
                }
            }
        }
        buf.extend_from_slice(b"\x1b[?2026l");
        self.out.write_all(buf)?;
        self.out.flush()?;
        self.last_bytes = buf.len();
        self.last_cells = changed;
        match &mut self.prev {
            Some(p) if !full => p.clone_from(f),
            _ => self.prev = Some(f.clone()),
        }
        Ok(())
    }

    /// Where key releases come from, for display.
    pub fn key_ups(&self) -> &'static str {
        if self.real_releases {
            "terminal"
        } else if self.polled_releases {
            "keyboard poll"
        } else {
            "none yet"
        }
    }

    /// Wait up to `timeout` for an input event.
    pub fn poll(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        if let Some(up) = self.synthesise_release() {
            return Ok(Some(up));
        }
        // While keys are held without real key-ups, wake often to check them.
        let timeout = if self.down.is_empty() || self.real_releases { timeout } else { timeout.min(Duration::from_millis(8)) };
        if !event::poll(timeout)? {
            return Ok(None);
        }
        Ok(match event::read()? {
            CEvent::Key(k) => {
                let ev = translate(k.code, k.modifiers);
                match k.kind {
                    KeyEventKind::Release => {
                        self.real_releases = true;
                        self.down.clear();
                        ev.map(|e| Event::KeyUp(e.key))
                    }
                    KeyEventKind::Repeat => ev.map(|e| Event::Key(KeyEvent { repeat: true, ..e })),
                    KeyEventKind::Press => {
                        if let Some(e) = ev {
                            if !self.real_releases && !self.down.contains(&e.key) && winkeys::trackable(e.key) {
                                self.down.push(e.key);
                            }
                        }
                        ev.map(Event::Key)
                    }
                }
            }
            CEvent::Resize(w, h) => Some(Event::Resize(w, h)),
            CEvent::Mouse(m) => {
                let b = |b: CButton| match b {
                    CButton::Left => MouseButton::Left,
                    CButton::Right => MouseButton::Right,
                    CButton::Middle => MouseButton::Middle,
                };
                let kind = match m.kind {
                    MouseEventKind::Down(x) => Some(MouseKind::Down(b(x))),
                    MouseEventKind::Up(x) => Some(MouseKind::Up(b(x))),
                    MouseEventKind::Drag(x) => Some(MouseKind::Drag(b(x))),
                    MouseEventKind::Moved => Some(MouseKind::Move),
                    MouseEventKind::ScrollUp => Some(MouseKind::ScrollUp),
                    MouseEventKind::ScrollDown => Some(MouseKind::ScrollDown),
                    _ => None,
                };
                kind.map(|kind| {
                    Event::Mouse(MouseEvent {
                        kind,
                        x: m.column as i32,
                        y: m.row as i32,
                        ctrl: m.modifiers.contains(KeyModifiers::CONTROL),
                        shift: m.modifiers.contains(KeyModifiers::SHIFT),
                        alt: m.modifiers.contains(KeyModifiers::ALT),
                    })
                })
            }
            _ => None,
        })
    }
}

impl Terminal {
    /// Some terminals never report key-ups. On Windows, ask the keyboard
    /// itself whether a key we saw go down is still held.
    fn synthesise_release(&mut self) -> Option<Event> {
        if self.real_releases {
            return None;
        }
        let k = self.down.iter().position(|&k| !winkeys::held(k))?;
        self.polled_releases = true;
        Some(Event::KeyUp(self.down.remove(k)))
    }
}

/// Physical key state, for terminals that do not report key releases.
#[cfg(windows)]
mod winkeys {
    use crate::input::Key;

    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vkey: i32) -> i16;
    }

    /// Virtual-key codes that might produce this key.
    fn codes(k: Key) -> [i32; 2] {
        match k {
            Key::Left => [0x25, 0x64],
            Key::Up => [0x26, 0x68],
            Key::Right => [0x27, 0x66],
            Key::Down => [0x28, 0x62],
            Key::Home => [0x24, 0x67],
            Key::End => [0x23, 0x61],
            Key::PageUp => [0x21, 0x69],
            Key::PageDown => [0x22, 0x63],
            Key::Char(c) if c.is_ascii_digit() => [c as i32, 0x60 + (c as i32 - '0' as i32)],
            Key::Char(c) if c.is_ascii_alphabetic() => [c.to_ascii_uppercase() as i32, 0],
            _ => [0, 0],
        }
    }

    pub fn trackable(k: Key) -> bool {
        codes(k)[0] != 0
    }

    pub fn held(k: Key) -> bool {
        codes(k).iter().any(|&vk| vk != 0 && unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0)
    }
}

#[cfg(not(windows))]
mod winkeys {
    use crate::input::Key;
    pub fn trackable(_: Key) -> bool {
        false
    }
    pub fn held(_: Key) -> bool {
        true
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        restore();
    }
}

fn translate(code: KeyCode, m: KeyModifiers) -> Option<KeyEvent> {
    let key = match code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };
    Some(KeyEvent {
        key,
        ctrl: m.contains(KeyModifiers::CONTROL),
        alt: m.contains(KeyModifiers::ALT),
        shift: m.contains(KeyModifiers::SHIFT),
        repeat: false,
    })
}

