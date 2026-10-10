//! Minds: who a creature is loyal to, what it does with its day, who it has
//! a grudge against, and what it is doing right now.
//!
//! A creature's behaviour is chosen fresh every so often (see `ai`): danger
//! first (fight or flee), then whichever need is most pressing (thirst,
//! hunger, sleep), then its routine for this hour of the day, then idling.

use crate::geom::Pos;
use crate::things::ThingId;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    /// Decides for itself.
    Ai,
    /// Driven by the player: needs and wounds still apply, but nothing is
    /// decided for it.
    Player,
}

/// Something to do at a time of day.
#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    /// Go to bed (or lie down here).
    Sleep { at: Option<Pos> },
    /// Stay near a place, pottering about (a smithy, a field, a shop).
    Work { at: Pos, radius: i32 },
    /// Stroll about an area.
    Wander { around: Pos, radius: i32 },
    /// Go somewhere and stay put (a guard post, a pew).
    Stay { at: Pos },
}

/// One entry in a routine: from one hour to another (it may wrap past
/// midnight: 22 to 6).
#[derive(Clone, Debug, PartialEq)]
pub struct Duty {
    pub from: f32,
    pub to: f32,
    pub task: Task,
}

/// A daily routine. The first duty that covers the hour applies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Routine {
    pub duties: Vec<Duty>,
}

impl Routine {
    pub fn new() -> Routine {
        Routine::default()
    }

    /// Builder: `Routine::new().with(22.0, 6.0, Task::Sleep { .. })`.
    pub fn with(mut self, from: f32, to: f32, task: Task) -> Routine {
        self.duties.push(Duty { from, to, task });
        self
    }

    /// What to be doing at this hour, if anything.
    pub fn now(&self, hour: f32) -> Option<&Task> {
        self.duties
            .iter()
            .find(|d| if d.from <= d.to { hour >= d.from && hour < d.to } else { hour >= d.from || hour < d.to })
            .map(|d| &d.task)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grudge {
    pub who: ThingId,
    /// 0..1 and more; it cools with time.
    pub anger: f32,
    /// Where it was last seen.
    pub seen: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Goal {
    /// Do nothing until this tick.
    Idle(u64),
    /// Walk somewhere (for the routine, or to wander).
    Go(Pos),
    Eat(ThingId),
    /// Graze where it stands, or walk to grass first.
    Graze(Pos),
    /// Walk to `stand`, then drink from the liquid at `water`.
    Drink { stand: Pos, water: Pos },
    /// Go to bed (or lie down where it is) and sleep.
    Sleep(Option<Pos>),
    Attack(ThingId),
    Flee(ThingId),
    /// Get out of fire or smoke.
    Escape,
}

#[derive(Clone, Debug)]
pub struct Mind {
    pub control: Control,
    pub routine: Routine,
    /// Where it lives: animals without a routine wander near it.
    pub home: Option<Pos>,
    /// Creatures of the same faction defend each other.
    pub faction: u16,
    pub goal: Goal,
    /// The route being followed, reversed (next step last).
    pub path: Vec<Pos>,
    pub grudges: Vec<Grudge>,
    pub asleep: bool,
    /// Next tick it may act, checks for danger, and reconsiders everything.
    pub next_act: u64,
    pub next_think: u64,
    pub next_plan: u64,
    /// Something happened (it was hit): reconsider at once.
    pub alarmed: bool,
    /// Steps in a row that were blocked.
    pub stuck: u8,
    /// Don't search for food or water again before this tick (none was found).
    pub no_food_until: u64,
    pub no_water_until: u64,
}

impl Mind {
    pub fn new(control: Control, faction: u16) -> Mind {
        Mind {
            control,
            routine: Routine::new(),
            home: None,
            faction,
            goal: Goal::Idle(0),
            path: Vec::new(),
            grudges: Vec::new(),
            asleep: false,
            next_act: 0,
            next_think: 0,
            next_plan: 0,
            alarmed: false,
            stuck: 0,
            no_food_until: 0,
            no_water_until: 0,
        }
    }

    /// Be angry at `who`, more so than before.
    pub fn resent(&mut self, who: ThingId, anger: f32, seen: Pos) {
        if let Some(g) = self.grudges.iter_mut().find(|g| g.who == who) {
            g.anger = (g.anger + anger).min(3.0);
            g.seen = seen;
        } else {
            self.grudges.push(Grudge { who, anger: anger.min(3.0), seen });
        }
        self.alarmed = true;
    }

    pub fn anger_at(&self, who: ThingId) -> f32 {
        self.grudges.iter().find(|g| g.who == who).map_or(0.0, |g| g.anger)
    }
}
