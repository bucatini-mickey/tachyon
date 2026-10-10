//! Backend-neutral key input. The terminal backend translates its events into
//! these; a web or phone backend would do the same.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    F(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct KeyEvent {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// An auto-repeat of a key that is being held down (where the backend can tell).
    pub repeat: bool,
}

impl KeyEvent {
    pub fn plain(key: Key) -> KeyEvent {
        KeyEvent { key, ctrl: false, alt: false, shift: false, repeat: false }
    }

    /// The character typed, if this is a printable key without Ctrl/Alt.
    pub fn char(&self) -> Option<char> {
        match self.key {
            Key::Char(c) if !self.ctrl && !self.alt => Some(c),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MouseKind {
    Down(MouseButton),
    Up(MouseButton),
    /// Moved with a button held.
    Drag(MouseButton),
    /// Moved with no button held.
    Move,
    ScrollUp,
    ScrollDown,
}

/// A mouse event at a screen cell (column, row).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub x: i32,
    pub y: i32,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    /// A key was let go. Not every terminal reports this.
    KeyUp(Key),
    Resize(u16, u16),
}

/// Turns direction keys into steps the way a player expects: a tap is one
/// step, holding a key walks at a steady pace, and holding two keys at once
/// (Up and Right) walks diagonally.
///
/// Feed it presses and releases, and call [`DirPad::update`] every frame.
/// A press waits `chord` before stepping, so two keys pressed together count
/// as one diagonal step. Terminals that never report releases still work: each
/// press (including the terminal's own key repeat) is then one step.
#[derive(Clone, Debug)]
pub struct DirPad {
    /// Time allowed for a second key to join a press.
    pub chord: Duration,
    /// Delay before a held key starts walking, and the pace after that.
    pub first_repeat: Duration,
    pub repeat: Duration,
    held: Vec<(Key, (i32, i32))>,
    /// Directions pressed since the last step (a quick tap may already be released).
    tapped: (i32, i32),
    next: Option<Instant>,
    steps: u32,
    releases: bool,
}

impl Default for DirPad {
    fn default() -> Self {
        DirPad {
            chord: Duration::from_millis(70),
            first_repeat: Duration::from_millis(220),
            repeat: Duration::from_millis(110),
            held: Vec::new(),
            tapped: (0, 0),
            next: None,
            steps: 0,
            releases: false,
        }
    }
}

impl DirPad {
    pub fn new() -> DirPad {
        DirPad::default()
    }

    /// A key meaning direction `dir` went down.
    pub fn press(&mut self, key: Key, dir: (i32, i32), now: Instant) {
        if self.held.iter().any(|&(k, _)| k == key) {
            if self.releases {
                return; // the terminal's own repeat; we keep our own pace
            }
        } else {
            // A key joining one already held (Right while walking Up): take the
            // diagonal step soon, rather than waiting for the next repeat.
            let joining = !self.held.is_empty();
            self.held.push((key, dir));
            if joining {
                if let Some(n) = self.next {
                    self.next = Some(n.min(now + self.chord));
                }
            }
        }
        self.tapped = (self.tapped.0 + dir.0, self.tapped.1 + dir.1);
        if self.next.is_none() {
            self.next = Some(now + self.chord);
            self.steps = 0;
        }
    }

    /// A key went up. Any key may be passed; non-direction keys are ignored.
    pub fn release(&mut self, key: Key) {
        self.releases = true;
        self.held.retain(|&(k, _)| k != key);
    }

    /// Has a key-up ever arrived? Without them, holding keys cannot work and
    /// each press is one step.
    pub fn sees_releases(&self) -> bool {
        self.releases
    }

    /// Forget everything held (on losing focus, or switching modes).
    pub fn clear(&mut self) {
        self.held.clear();
        self.tapped = (0, 0);
        self.next = None;
    }

    /// The step to take now, if any: each axis is -1, 0 or 1.
    pub fn update(&mut self, now: Instant) -> Option<(i32, i32)> {
        let due = self.next?;
        if now < due {
            return None;
        }
        let (mut dx, mut dy) = self.tapped;
        for &(_, (x, y)) in &self.held {
            dx += x;
            dy += y;
        }
        self.tapped = (0, 0);
        if !self.releases {
            self.held.clear(); // can't tell when keys go up: one step per press
        }
        self.next = if self.held.is_empty() {
            None
        } else {
            self.steps += 1;
            Some(now + if self.steps == 1 { self.first_repeat } else { self.repeat })
        };
        let step = (dx.signum(), dy.signum());
        (step != (0, 0)).then_some(step)
    }
}
