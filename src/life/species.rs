//! Species: everything creatures of one kind share. A game registers its
//! own; [`human`], [`dog`], [`cat`], [`rat`] and [`sheep`] are ready-made
//! starting points to copy and change.

use super::body::{BodyPlan, DamageKind, Does, Tissue};
use super::needs::NeedRates;
use crate::color::Rgb;
use crate::sound::{DOG_HEARING, HUMAN_HEARING};
use crate::things::KindId;

/// A way of hurting something.
#[derive(Clone, Debug, PartialEq)]
pub struct Attack {
    /// "punch": what you do.
    pub name: String,
    /// "punches": what they do.
    pub verb: String,
    pub kind: DamageKind,
    pub force: f32,
    /// What the body needs to do it (hands to punch, jaws to bite).
    pub uses: Does,
}

impl Attack {
    pub fn new(name: &str, verb: &str, kind: DamageKind, force: f32, uses: Does) -> Attack {
        Attack { name: name.into(), verb: verb.into(), kind, force, uses }
    }
}

/// How a creature tends to behave, each 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Temperament {
    /// Quick to fight rather than flee.
    pub aggression: f32,
    /// Stands its ground against bigger things.
    pub courage: f32,
    /// Joins in when one of its own is attacked.
    pub loyalty: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Diet {
    pub plants: bool,
    pub meat: bool,
    /// Eats the grass it stands on.
    pub grazes: bool,
}

#[derive(Clone, Debug)]
pub struct Species {
    pub name: String,
    pub plan: BodyPlan,
    /// kg.
    pub mass: f32,
    pub attacks: Vec<Attack>,
    pub temperament: Temperament,
    pub diet: Diet,
    pub needs: NeedRates,
    /// Walking speed in metres per second when fit (it runs at twice the pace).
    /// A human walks about 1.3 m/s. The AI converts it with the clock, so a long
    /// tick means several cells per tick.
    pub speed: f32,
    pub blood: Rgb,
    /// The thing kind its body becomes when it dies.
    pub corpse: Option<KindId>,
    /// Species it hunts when hungry.
    pub prey: Vec<String>,
    /// Species it keeps away from.
    pub wary_of: Vec<String>,
    /// Sleeps by day and is about at night.
    pub nocturnal: bool,
    /// Can swim (others flounder: they tire and their breath runs short).
    pub swims: bool,
    /// How far below the background noise it still picks a sound out, dB
    /// (lower is keener): human `sound::HUMAN_HEARING` (-10), dog
    /// `sound::DOG_HEARING` (-20). Also sets how easily it is woken.
    pub hearing: f32,
}

pub const RED_BLOOD: Rgb = Rgb(150, 18, 22);

fn base(name: &str, plan: BodyPlan, mass: f32) -> Species {
    Species {
        name: name.into(),
        plan,
        mass,
        attacks: Vec::new(),
        temperament: Temperament { aggression: 0.3, courage: 0.4, loyalty: 0.3 },
        diet: Diet { plants: true, meat: true, grazes: false },
        needs: NeedRates::default(),
        speed: 1.3,
        blood: RED_BLOOD,
        corpse: None,
        prey: Vec::new(),
        wary_of: Vec::new(),
        nocturnal: false,
        swims: true,
        hearing: HUMAN_HEARING,
    }
}

pub fn human() -> Species {
    let mut s = base("human", BodyPlan::humanoid(70.0), 70.0);
    s.attacks = vec![
        Attack::new("punch", "punches", DamageKind::Blunt, 12.0, Does::GRASP),
        Attack::new("kick", "kicks", DamageKind::Blunt, 18.0, Does::STAND),
    ];
    s
}

pub fn dog() -> Species {
    let mut s = base("dog", BodyPlan::quadruped(30.0, Some((Tissue::Fur, 4.0)), true), 30.0);
    s.attacks = vec![Attack::new("bite", "bites", DamageKind::Point, 26.0, Does::BITE)];
    s.temperament = Temperament { aggression: 0.6, courage: 0.8, loyalty: 0.9 };
    s.needs.comfort = (-5.0, 28.0);
    s.speed = 1.8;
    s.prey = vec!["rat".into()];
    s.hearing = DOG_HEARING;
    s
}

pub fn cat() -> Species {
    let mut s = base("cat", BodyPlan::quadruped(4.5, Some((Tissue::Fur, 3.0)), true), 4.5);
    s.attacks = vec![
        Attack::new("scratch", "scratches", DamageKind::Edge, 6.0, Does::STAND),
        Attack::new("bite", "bites", DamageKind::Point, 7.0, Does::BITE),
    ];
    s.temperament = Temperament { aggression: 0.4, courage: 0.3, loyalty: 0.0 };
    s.diet = Diet { plants: false, meat: true, grazes: false };
    s.needs.comfort = (0.0, 30.0);
    s.needs.fatigue = 2.0; // cats sleep a lot
    s.speed = 1.5;
    s.prey = vec!["rat".into()];
    s.wary_of = vec!["dog".into()];
    s.nocturnal = true;
    s.swims = false;
    s.hearing = -18.0; // cats hear nearly as well as dogs
    s
}

pub fn rat() -> Species {
    let mut s = base("rat", BodyPlan::quadruped(0.3, Some((Tissue::Fur, 1.0)), true), 0.3);
    s.attacks = vec![Attack::new("bite", "bites", DamageKind::Point, 2.5, Does::BITE)];
    s.temperament = Temperament { aggression: 0.15, courage: 0.1, loyalty: 0.0 };
    s.needs.comfort = (0.0, 32.0);
    s.needs.hunger = 3.0;
    s.speed = 1.0;
    s.wary_of = vec!["cat".into(), "dog".into(), "human".into()];
    s.nocturnal = true;
    s.hearing = -15.0;
    s
}

pub fn sheep() -> Species {
    let mut s = base("sheep", BodyPlan::quadruped(60.0, Some((Tissue::Fur, 25.0)), false), 60.0);
    s.attacks = vec![Attack::new("butt", "butts", DamageKind::Blunt, 16.0, Does::NOTHING)];
    s.temperament = Temperament { aggression: 0.05, courage: 0.05, loyalty: 0.2 };
    s.diet = Diet { plants: true, meat: false, grazes: true };
    s.needs.comfort = (-10.0, 26.0);
    s.speed = 0.9;
    s.wary_of = vec!["dog".into()];
    s
}
