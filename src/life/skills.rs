//! Skills: how good each creature is at each kind of work.
//!
//! The game registers its own skills with [`Life::add_skill`], the same way it
//! registers species; the engine ships none. A creature holds one proficiency
//! per registered skill, 0 (untrained) to 1 (master), in a plain vector
//! indexed by [`SkillId`]. In Python terms it is `creature.skills[COOKING]`.
//! Nothing reads these numbers yet: later steps of
//! `docs/design/SKILLS_AND_TIME.md` use them to set how long a job takes.

use super::{Creature, Life};

/// Which skill: an index into [`Life::skills`] and into every creature's
/// `skills`. A newtype like [`Span`](crate::clock::Span), so a species id or a
/// plain number cannot be passed by mistake. The `derive` line gives it the
/// everyday traits: it copies like a number, compares with `==` and can be a
/// `HashMap` key.
///
/// Ids belong to the [`Life`] that issued them. Two registries both start at
/// 0, so an id from one means nothing in the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkillId(pub u16);

/// What the game says about one skill.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillDef {
    pub name: String,
    /// How much mastery matters for this skill's jobs (the `spread` of
    /// section 4 of the design note). 1.0 is Broadside's 1.6x to 0.6x range;
    /// more for surgery or lock picking, 0 to ignore skill.
    pub spread: f32,
    /// How fast use improves it, as a multiplier on the learning rule of
    /// section 7.2 (1.0 = the standard pace; the rule itself comes later).
    pub learn_rate: f32,
}

impl SkillDef {
    /// A skill with the defaults: `spread` 1.0 and `learn_rate` 1.0.
    pub fn new(name: &str) -> SkillDef {
        SkillDef { name: name.into(), spread: 1.0, learn_rate: 1.0 }
    }
}

impl Life {
    /// Register a skill and return its id. Creatures already added get 0
    /// (untrained) for it, so this can be called at any time.
    pub fn add_skill(&mut self, def: SkillDef) -> SkillId {
        let id = SkillId(u16::try_from(self.skills.len()).expect("more than 65535 skills"));
        self.skills.push(def);
        for c in &mut self.creatures {
            c.skills.push(0.0);
        }
        id
    }

    pub fn skill_def(&self, id: SkillId) -> Option<&SkillDef> {
        self.skills.get(id.0 as usize)
    }

    pub fn find_skill(&self, name: &str) -> Option<SkillId> {
        self.skills.iter().position(|s| s.name == name).map(|i| SkillId(i as u16))
    }
}

impl Creature {
    /// Proficiency 0..1; 0 for an id this creature's registry does not have.
    pub fn skill(&self, id: SkillId) -> f32 {
        self.skills.get(id.0 as usize).copied().unwrap_or(0.0)
    }

    /// Set a proficiency, clamped to 0..1. Returns false (and changes nothing)
    /// if the id is not in this creature's registry.
    pub fn set_skill(&mut self, id: SkillId, value: f32) -> bool {
        match self.skills.get_mut(id.0 as usize) {
            Some(slot) => {
                // `f32::clamp` leaves a NaN as NaN, so call it untrained instead.
                *slot = if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) };
                true
            }
            None => false,
        }
    }
}
