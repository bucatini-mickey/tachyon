//! Bodies: what a creature is made of, and what has happened to it.
//!
//! A [`BodyPlan`] is the anatomy a species shares: a tree of parts (a forearm
//! hangs from an upper arm, a hand from the forearm; the heart sits inside
//! the chest), each built of tissue layers from the outside in (skin, fat,
//! muscle, bone). A [`Body`] is one creature's state: its wounds, its blood,
//! and which parts it has lost.
//!
//! Damage goes in layer by layer. An edge cuts each layer until its force is
//! spent; a point is narrower but reaches deeper, and glances off bone; a
//! blunt blow bruises soft tissue, breaks bone and shakes what lies inside; a
//! burn sears the outside. Whatever gets through a part's layers reaches an
//! organ inside it, and a limb cut clean through is severed. Wounds bleed
//! until they clot, hurt, cost the part some of its use, and heal with time;
//! bruises change colour as they heal.

use crate::color::Rgb;
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tissue {
    Skin,
    Fur,
    Scales,
    Fat,
    Muscle,
    Bone,
    Organ,
}

/// How a tissue behaves. The numbers are for a human-sized body: layer
/// thickness scales how hard a layer is to cut, and the plan's `toughness`
/// scales the blunt and burn thresholds.
#[derive(Clone, Copy, Debug)]
pub struct TissueProps {
    /// Force an edge needs per mm of it.
    pub cut: f32,
    /// Share of a blunt blow it soaks up before passing it on.
    pub cushion: f32,
    /// ml per second an open wound of full severity in it bleeds.
    pub bleed: f32,
    pub pain: f32,
    /// How much a wound of full severity in it costs the part's use.
    pub weight: f32,
    /// Seconds for its bleeding to clot to a third.
    pub clot: f32,
}

impl Tissue {
    pub fn props(self) -> TissueProps {
        let (cut, cushion, bleed, pain, weight, clot) = match self {
            Tissue::Skin => (1.5, 0.05, 0.4, 1.0, 0.15, 90.0),
            Tissue::Fur => (0.8, 0.25, 0.0, 0.0, 0.0, 1.0),
            Tissue::Scales => (4.0, 0.15, 0.0, 0.2, 0.05, 1.0),
            Tissue::Fat => (0.2, 0.25, 0.3, 0.3, 0.05, 120.0),
            Tissue::Muscle => (0.5, 0.3, 1.5, 0.8, 0.5, 240.0),
            Tissue::Bone => (8.0, 0.1, 0.6, 1.2, 0.8, 400.0),
            Tissue::Organ => (0.3, 0.05, 4.0, 1.2, 1.0, 600.0),
        };
        TissueProps { cut, cushion, bleed, pain, weight, clot }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tissue::Skin => "skin",
            Tissue::Fur => "fur",
            Tissue::Scales => "scales",
            Tissue::Fat => "fat",
            Tissue::Muscle => "muscle",
            Tissue::Bone => "bone",
            Tissue::Organ => "organ",
        }
    }

    /// An outer covering that takes blows but is not itself hurt by them.
    fn covering(self) -> bool {
        matches!(self, Tissue::Fur | Tissue::Scales)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageKind {
    /// Fists, clubs, falls, the shock of a blast.
    Blunt,
    /// Blades and claws.
    Edge,
    /// Teeth, spears, arrows.
    Point,
    /// Fire and great heat.
    Burn,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WoundKind {
    Bruise,
    Cut,
    Puncture,
    Fracture,
    Burn,
}

/// What a part does for the body, as bit flags: `Does::GRASP | Does::LIMB`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Does(pub u16);

impl Does {
    pub const NOTHING: Does = Does(0);
    /// The brain: consciousness.
    pub const THINK: Does = Does(1);
    /// The heart.
    pub const PUMP: Does = Does(1 << 1);
    /// Lungs.
    pub const BREATHE: Does = Does(1 << 2);
    /// Feet and paws: walking.
    pub const STAND: Does = Does(1 << 3);
    /// Hands: holding things, punching, wielding.
    pub const GRASP: Does = Does(1 << 4);
    pub const SEE: Does = Does(1 << 5);
    /// Jaws.
    pub const BITE: Does = Does(1 << 6);
    /// A segment of a limb: only as useful as the segment it hangs from, and
    /// it can be cut off.
    pub const LIMB: Does = Does(1 << 7);
    /// Losing it kills.
    pub const VITAL: Does = Does(1 << 8);

    #[inline]
    pub fn has(self, o: Does) -> bool {
        o.0 != 0 && self.0 & o.0 == o.0
    }
}

// `a | b` combines flags. (Implementing the operator trait is how Rust does
// operator overloading.)
impl std::ops::BitOr for Does {
    type Output = Does;
    fn bitor(self, o: Does) -> Does {
        Does(self.0 | o.0)
    }
}

/// Which band of the body a part is in: a rat can only reach the legs of a
/// standing man.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Height {
    Low,
    Mid,
    High,
}

#[derive(Clone, Debug)]
pub struct PartDef {
    pub name: String,
    pub parent: Option<usize>,
    /// An organ inside its parent, reached only through the parent's layers.
    pub inside: bool,
    /// Relative chance of being hit (roughly its surface area).
    pub size: f32,
    pub height: Height,
    /// Tissue layers from the outside in, with thickness in mm.
    pub layers: Vec<(Tissue, f32)>,
    pub does: Does,
    /// Bleeding multiplier: the neck's great vessels, the heart.
    pub vascular: f32,
}

#[derive(Clone, Debug)]
pub struct BodyPlan {
    pub name: String,
    pub parts: Vec<PartDef>,
    /// Blood volume in ml.
    pub blood: f32,
    /// Scales the blunt and burn thresholds (1 for a human).
    pub toughness: f32,
}

impl BodyPlan {
    /// An empty plan for a body of `mass` kg (blood and toughness follow).
    pub fn new(name: &str, mass: f32) -> BodyPlan {
        BodyPlan { name: name.into(), parts: Vec::new(), blood: 70.0 * mass, toughness: scale_for(mass) }
    }

    /// Add an outer part. Thicknesses are for a 70 kg body and are scaled to
    /// this one's size.
    pub fn part(&mut self, name: &str, parent: Option<usize>, size: f32, height: Height, layers: &[(Tissue, f32)], does: Does) -> usize {
        let s = self.toughness;
        self.parts.push(PartDef {
            name: name.into(),
            parent,
            inside: false,
            size,
            height,
            layers: layers.iter().map(|&(t, mm)| (t, mm * s)).collect(),
            does,
            vascular: 1.0,
        });
        self.parts.len() - 1
    }

    /// Add an organ inside `inside`.
    pub fn organ(&mut self, name: &str, inside: usize, size: f32, mm: f32, does: Does) -> usize {
        let h = self.parts[inside].height;
        let i = self.part(name, Some(inside), size, h, &[(Tissue::Organ, mm)], does);
        self.parts[i].inside = true;
        i
    }

    pub fn vascular(&mut self, part: usize, v: f32) {
        self.parts[part].vascular = v;
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.name == name)
    }

    /// Is `part` below `ancestor` in the tree (or the same part)?
    pub fn descends(&self, mut part: usize, ancestor: usize) -> bool {
        loop {
            if part == ancestor {
                return true;
            }
            match self.parts[part].parent {
                Some(p) => part = p,
                None => return false,
            }
        }
    }

    /// A person (or anything shaped like one) of `mass` kg.
    pub fn humanoid(mass: f32) -> BodyPlan {
        use Height::*;
        use Tissue::*;
        let mut b = BodyPlan::new("humanoid", mass);
        let chest = b.part("chest", None, 18.0, Mid, &[(Skin, 2.0), (Fat, 6.0), (Muscle, 20.0), (Bone, 10.0)], Does::NOTHING);
        b.organ("heart", chest, 2.0, 30.0, Does::PUMP | Does::VITAL);
        b.organ("left lung", chest, 5.0, 40.0, Does::BREATHE);
        b.organ("right lung", chest, 5.0, 40.0, Does::BREATHE);
        let heart = b.find("heart").unwrap();
        b.vascular(heart, 8.0);
        let belly = b.part("belly", Some(chest), 14.0, Mid, &[(Skin, 2.0), (Fat, 15.0), (Muscle, 15.0)], Does::NOTHING);
        b.organ("liver", belly, 4.0, 40.0, Does::NOTHING);
        b.organ("stomach", belly, 3.0, 20.0, Does::NOTHING);
        b.organ("guts", belly, 6.0, 30.0, Does::NOTHING);
        let neck = b.part("neck", Some(chest), 3.0, High, &[(Skin, 2.0), (Fat, 2.0), (Muscle, 15.0), (Bone, 8.0)], Does::LIMB);
        b.vascular(neck, 3.0);
        b.organ("throat", neck, 2.0, 10.0, Does::NOTHING);
        let head = b.part("head", Some(neck), 7.0, High, &[(Skin, 3.0), (Fat, 1.0), (Muscle, 3.0), (Bone, 7.0)], Does::NOTHING);
        b.organ("brain", head, 6.0, 40.0, Does::THINK | Does::VITAL);
        b.part("left eye", Some(head), 0.4, High, &[(Organ, 10.0)], Does::SEE);
        b.part("right eye", Some(head), 0.4, High, &[(Organ, 10.0)], Does::SEE);
        b.part("jaw", Some(head), 1.0, High, &[(Skin, 2.0), (Muscle, 6.0), (Bone, 6.0)], Does::BITE);
        for side in ["left", "right"] {
            let ua = b.part(&format!("{side} upper arm"), Some(chest), 6.0, Mid, &[(Skin, 2.0), (Fat, 4.0), (Muscle, 25.0), (Bone, 8.0)], Does::LIMB);
            let fa = b.part(&format!("{side} forearm"), Some(ua), 5.0, Mid, &[(Skin, 2.0), (Fat, 3.0), (Muscle, 20.0), (Bone, 7.0)], Does::LIMB);
            b.part(&format!("{side} hand"), Some(fa), 3.0, Mid, &[(Skin, 2.0), (Fat, 1.0), (Muscle, 6.0), (Bone, 4.0)], Does::LIMB | Does::GRASP);
            let th = b.part(&format!("{side} thigh"), Some(belly), 10.0, Low, &[(Skin, 2.0), (Fat, 8.0), (Muscle, 40.0), (Bone, 12.0)], Does::LIMB);
            b.vascular(th, 1.5);
            let sh = b.part(&format!("{side} shin"), Some(th), 8.0, Low, &[(Skin, 2.0), (Fat, 3.0), (Muscle, 25.0), (Bone, 10.0)], Does::LIMB);
            b.part(&format!("{side} foot"), Some(sh), 4.0, Low, &[(Skin, 3.0), (Fat, 2.0), (Muscle, 6.0), (Bone, 5.0)], Does::LIMB | Does::STAND);
        }
        b
    }

    /// A four-legged animal of `mass` kg with the given coat (fur, scales,
    /// or bare skin) over its skin.
    pub fn quadruped(mass: f32, coat: Option<(Tissue, f32)>, tail: bool) -> BodyPlan {
        use Height::*;
        use Tissue::*;
        let mut b = BodyPlan::new("quadruped", mass);
        let with = |layers: &[(Tissue, f32)]| -> Vec<(Tissue, f32)> { coat.into_iter().chain(layers.iter().copied()).collect() };
        let body = b.part("body", None, 30.0, Mid, &with(&[(Skin, 2.0), (Fat, 8.0), (Muscle, 20.0), (Bone, 9.0)]), Does::NOTHING);
        b.organ("heart", body, 2.0, 30.0, Does::PUMP | Does::VITAL);
        b.organ("lungs", body, 8.0, 40.0, Does::BREATHE);
        b.organ("guts", body, 9.0, 30.0, Does::NOTHING);
        let heart = b.find("heart").unwrap();
        b.vascular(heart, 8.0);
        let neck = b.part("neck", Some(body), 4.0, Mid, &with(&[(Skin, 2.0), (Fat, 2.0), (Muscle, 15.0), (Bone, 8.0)]), Does::LIMB);
        b.vascular(neck, 3.0);
        b.organ("throat", neck, 2.0, 10.0, Does::NOTHING);
        let head = b.part("head", Some(neck), 7.0, Mid, &with(&[(Skin, 2.0), (Muscle, 3.0), (Bone, 7.0)]), Does::NOTHING);
        b.organ("brain", head, 5.0, 30.0, Does::THINK | Does::VITAL);
        b.part("left eye", Some(head), 0.4, Mid, &[(Organ, 10.0)], Does::SEE);
        b.part("right eye", Some(head), 0.4, Mid, &[(Organ, 10.0)], Does::SEE);
        b.part("jaw", Some(head), 2.0, Mid, &[(Skin, 2.0), (Muscle, 8.0), (Bone, 7.0)], Does::BITE);
        for (end, size) in [("front", 5.0), ("hind", 6.0)] {
            for side in ["left", "right"] {
                let leg = b.part(&format!("{end} {side} leg"), Some(body), size, Low, &with(&[(Skin, 2.0), (Fat, 3.0), (Muscle, 25.0), (Bone, 9.0)]), Does::LIMB);
                b.part(&format!("{end} {side} paw"), Some(leg), 2.0, Low, &with(&[(Skin, 3.0), (Muscle, 5.0), (Bone, 5.0)]), Does::LIMB | Does::STAND);
            }
        }
        if tail {
            b.part("tail", Some(body), 2.0, Mid, &with(&[(Skin, 2.0), (Muscle, 6.0), (Bone, 4.0)]), Does::LIMB);
        }
        b
    }
}

/// Size factor for layer thickness and toughness: a cube-root of mass, so a
/// 4 kg cat has tissues about two-fifths as thick as a man's.
fn scale_for(mass: f32) -> f32 {
    (mass.max(0.01) / 70.0).cbrt()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wound {
    pub kind: WoundKind,
    pub tissue: Tissue,
    /// 0..1.
    pub severity: f32,
    /// ml per second it is bleeding now.
    pub bleed: f32,
    /// Seconds since it was made (or last made worse).
    pub age: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartState {
    pub wounds: Vec<Wound>,
    /// Cut off (or hanging from something that was).
    pub severed: bool,
}

/// What one blow did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Injury {
    /// The part struck.
    pub part: usize,
    /// The organ inside it that was reached, if any.
    pub organ: Option<usize>,
    /// (part, kind, tissue, severity) for each wound made.
    pub wounds: Vec<(usize, WoundKind, Tissue, f32)>,
    /// The part cut off, if one was.
    pub severed: Option<usize>,
}

impl Injury {
    /// The worst wound made, if any.
    pub fn worst(&self) -> Option<(usize, WoundKind, Tissue, f32)> {
        self.wounds.iter().copied().max_by(|a, b| (a.3 * a.2.props().weight.max(0.1)).total_cmp(&(b.3 * b.2.props().weight.max(0.1))))
    }
}

/// What a body can do right now, each 0..1 (pain and blood are raw).
#[derive(Clone, Copy, Debug, Default)]
pub struct Capacities {
    pub conscious: f32,
    pub moving: f32,
    pub grasp: f32,
    pub sight: f32,
    pub breathing: f32,
    pub pumping: f32,
    pub biting: f32,
    /// Total pain (about 1 per bad wound; above 8 or so a creature is stunned).
    pub pain: f32,
    /// Share of full blood volume left.
    pub blood: f32,
}

/// Why something died.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cause {
    /// A vital part was destroyed.
    Destroyed(usize),
    /// A part with a vital part in it was cut off (beheading).
    Severed(usize),
    BledOut,
    Burned,
    Drowned,
    Suffocated,
    Froze,
    Overheated,
    Starved,
    Thirst,
}

impl Cause {
    pub fn describe(self, plan: &BodyPlan) -> String {
        match self {
            Cause::Destroyed(p) => format!("{} destroyed", plan.parts[p].name),
            Cause::Severed(p) => format!("{} cut off", plan.parts[p].name),
            Cause::BledOut => "bled to death".into(),
            Cause::Burned => "burned to death".into(),
            Cause::Drowned => "drowned".into(),
            Cause::Suffocated => "suffocated".into(),
            Cause::Froze => "froze to death".into(),
            Cause::Overheated => "died of heatstroke".into(),
            Cause::Starved => "starved to death".into(),
            Cause::Thirst => "died of thirst".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub parts: Vec<PartState>,
    /// ml.
    pub blood: f32,
    pub max_blood: f32,
}

impl Body {
    pub fn new(plan: &BodyPlan) -> Body {
        Body { parts: vec![PartState::default(); plan.parts.len()], blood: plan.blood, max_blood: plan.blood }
    }

    /// A random outer part a blow could land on, weighted by size, no higher
    /// than `reach`.
    pub fn pick_part(&self, plan: &BodyPlan, reach: Height, rng: &mut Rng) -> Option<usize> {
        let ok = |i: usize| !plan.parts[i].inside && !self.parts[i].severed && plan.parts[i].height <= reach;
        let total: f32 = (0..plan.parts.len()).filter(|&i| ok(i)).map(|i| plan.parts[i].size).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = rng.f32() * total;
        for i in (0..plan.parts.len()).filter(|&i| ok(i)) {
            r -= plan.parts[i].size;
            if r <= 0.0 {
                return Some(i);
            }
        }
        (0..plan.parts.len()).rfind(|&i| ok(i))
    }

    /// Land a blow of `force` on `part`. See the module notes.
    pub fn strike(&mut self, plan: &BodyPlan, part: usize, kind: DamageKind, force: f32, rng: &mut Rng) -> Injury {
        let mut inj = Injury { part, ..Injury::default() };
        if self.parts[part].severed {
            return inj;
        }
        let t = plan.toughness;
        let size = self.max_blood / 5000.0;
        let mut f = force;
        let mut at = part;
        loop {
            let def = &plan.parts[at];
            let mut through = true;
            for &(tissue, mm) in &def.layers {
                if f <= 0.0 {
                    through = false;
                    break;
                }
                let p = tissue.props();
                let mut made: Option<(WoundKind, f32)> = None;
                match kind {
                    DamageKind::Edge | DamageKind::Point => {
                        let point = kind == DamageKind::Point;
                        let mut resist = mm * p.cut * if point { 0.5 } else { 1.0 };
                        if point && tissue == Tissue::Bone {
                            resist *= 2.0; // points glance off bone
                        }
                        let depth = (f / resist.max(0.01)).min(1.0);
                        if !tissue.covering() {
                            let (wk, width) = match (tissue, point) {
                                (Tissue::Bone, _) => (WoundKind::Fracture, if point { 0.6 } else { 1.0 }),
                                (_, true) => (WoundKind::Puncture, 0.45),
                                (_, false) => (WoundKind::Cut, 0.7),
                            };
                            made = Some((wk, depth * width));
                        }
                        f -= resist;
                        if depth < 1.0 {
                            through = false;
                        }
                    }
                    DamageKind::Blunt => {
                        let sev = match tissue {
                            Tissue::Skin => (f - 4.0 * t) / (40.0 * t),
                            Tissue::Muscle => (f - 15.0 * t) / (50.0 * t),
                            Tissue::Organ => (f - 10.0 * t) / (40.0 * t),
                            Tissue::Bone => {
                                let snap = mm * 3.0;
                                if f > snap {
                                    0.3 + 0.7 * (f - snap) / snap
                                } else {
                                    0.0
                                }
                            }
                            _ => 0.0,
                        };
                        if sev > 0.02 {
                            let wk = if tissue == Tissue::Bone { WoundKind::Fracture } else { WoundKind::Bruise };
                            made = Some((wk, sev.min(1.0)));
                        }
                        f *= 1.0 - p.cushion;
                    }
                    DamageKind::Burn => {
                        if matches!(tissue, Tissue::Bone | Tissue::Organ) {
                            through = false;
                            break;
                        }
                        let sev = (f - 2.0 * t) / (25.0 * t);
                        if sev > 0.02 {
                            made = Some((WoundKind::Burn, sev.min(1.0)));
                        }
                        f *= 0.35;
                    }
                }
                if let Some((wk, sev)) = made {
                    if sev > 0.02 {
                        let open = matches!(wk, WoundKind::Cut | WoundKind::Puncture);
                        let bleed = if open {
                            sev * p.bleed * def.vascular * size
                        } else if wk == WoundKind::Fracture || (wk == WoundKind::Bruise && tissue == Tissue::Organ) {
                            sev * p.bleed * 0.3 * def.vascular * size // inside: no stain
                        } else {
                            0.0
                        };
                        self.wound(at, Wound { kind: wk, tissue, severity: sev, bleed, age: 0.0 });
                        inj.wounds.push((at, wk, tissue, sev));
                    }
                }
            }
            let reached = through && f > 0.0;
            // A limb cut clean through comes off.
            if at == part && reached && kind == DamageKind::Edge && def.does.has(Does::LIMB) {
                self.sever(plan, part);
                inj.severed = Some(part);
            }
            // What gets through goes on into an organ inside.
            if at == part && reached && kind != DamageKind::Burn {
                let organs: Vec<usize> = (0..plan.parts.len()).filter(|&i| plan.parts[i].inside && plan.parts[i].parent == Some(part)).collect();
                let total: f32 = organs.iter().map(|&i| plan.parts[i].size).sum();
                if total > 0.0 {
                    let mut r = rng.f32() * total;
                    let pick = organs.iter().copied().find(|&i| {
                        r -= plan.parts[i].size;
                        r <= 0.0
                    });
                    if let Some(o) = pick.or(organs.last().copied()) {
                        inj.organ = Some(o);
                        at = o;
                        if kind == DamageKind::Blunt {
                            f *= 0.6;
                        }
                        continue;
                    }
                }
            }
            break;
        }
        inj
    }

    /// Add a wound, merging it into a like one already there.
    fn wound(&mut self, part: usize, w: Wound) {
        let list = &mut self.parts[part].wounds;
        if let Some(o) = list.iter_mut().find(|o| o.kind == w.kind && o.tissue == w.tissue) {
            o.severity = (o.severity + w.severity).min(1.0);
            o.bleed += w.bleed;
            o.age = 0.0;
        } else {
            list.push(w);
        }
    }

    /// Cut a part off, with everything hanging from it.
    pub fn sever(&mut self, plan: &BodyPlan, part: usize) {
        let size = self.max_blood / 5000.0;
        for i in 0..plan.parts.len() {
            if plan.descends(i, part) {
                self.parts[i].severed = true;
            }
        }
        // The stump bleeds hard: it is a wound on the parent.
        let stump = plan.parts[part].parent.unwrap_or(part);
        let bleed = 5.0 * plan.parts[part].vascular * size;
        self.wound(stump, Wound { kind: WoundKind::Cut, tissue: Tissue::Muscle, severity: 0.8, bleed, age: 0.0 });
    }

    /// How well a part works by itself, 0..1.
    pub fn own(&self, plan: &BodyPlan, part: usize) -> f32 {
        let _ = plan;
        let s = &self.parts[part];
        if s.severed {
            return 0.0;
        }
        let loss: f32 = s
            .wounds
            .iter()
            .map(|w| {
                let k = match w.kind {
                    WoundKind::Bruise => 0.4,
                    WoundKind::Burn => 0.7,
                    _ => 1.0,
                };
                w.severity * w.tissue.props().weight * k
            })
            .sum();
        (1.0 - loss).clamp(0.0, 1.0)
    }

    /// How well a part works, counting the limb segments it hangs from (a
    /// hand on a broken arm is little use).
    pub fn effective(&self, plan: &BodyPlan, part: usize) -> f32 {
        let mut v = self.own(plan, part);
        let mut at = part;
        while plan.parts[at].does.has(Does::LIMB) {
            match plan.parts[at].parent {
                Some(p) if plan.parts[p].does.has(Does::LIMB) => {
                    v *= self.own(plan, p);
                    at = p;
                }
                _ => break,
            }
        }
        v
    }

    pub fn pain(&self) -> f32 {
        let mut p = 0.0;
        for s in &self.parts {
            for w in &s.wounds {
                let k = match w.kind {
                    WoundKind::Burn | WoundKind::Fracture => 1.5,
                    WoundKind::Bruise => 0.5,
                    _ => 1.0,
                };
                // Pain dulls as a wound ages.
                p += w.severity * w.tissue.props().pain * k * (0.4 + 0.6 * (-w.age / 7200.0).exp());
            }
        }
        p
    }

    /// ml per second being lost.
    pub fn bleeding(&self) -> f32 {
        self.parts.iter().flat_map(|s| &s.wounds).map(|w| w.bleed).sum()
    }

    pub fn capacities(&self, plan: &BodyPlan) -> Capacities {
        let parts = || 0..plan.parts.len();
        let with = |d: Does| parts().filter(move |&i| plan.parts[i].does.has(d));
        let min_of = |d: Does| with(d).map(|i| self.effective(plan, i)).fold(None, |a: Option<f32>, v| Some(a.map_or(v, |a| a.min(v)))).unwrap_or(1.0);
        let max_of = |d: Does| with(d).map(|i| self.effective(plan, i)).fold(None, |a: Option<f32>, v| Some(a.map_or(v, |a| a.max(v)))).unwrap_or(1.0);
        let avg_of = |d: Does| {
            let v: Vec<f32> = with(d).map(|i| self.effective(plan, i)).collect();
            if v.is_empty() {
                (1.0, 1.0, 1)
            } else {
                (v.iter().sum::<f32>() / v.len() as f32, v.iter().copied().fold(1.0, f32::min), v.len())
            }
        };
        let think = min_of(Does::THINK);
        let pumping = min_of(Does::PUMP);
        let (breathing, _, _) = avg_of(Does::BREATHE);
        let blood = (self.blood / self.max_blood).clamp(0.0, 1.0);
        let pain = self.pain();
        let blood_f = ((blood - 0.45) / 0.3).clamp(0.0, 1.0);
        let pain_f = (1.0 - pain / 12.0).clamp(0.3, 1.0) * if pain > 8.0 { 0.5 } else { 1.0 };
        let conscious = (think * blood_f * pain_f * pumping.min(1.0) * (0.3 + 0.7 * breathing)).clamp(0.0, 1.0);
        // Walking: the average of the feet, pulled down by the worst one (a
        // man on one leg hops; a dog on three legs manages).
        let (avg, worst, n) = avg_of(Does::STAND);
        let mut moving = avg - (avg - worst) / n as f32;
        moving = moving.min(conscious.min(1.0) * 1.2).clamp(0.0, 1.0);
        Capacities {
            conscious,
            moving,
            grasp: max_of(Does::GRASP) * conscious.min(1.0),
            sight: max_of(Does::SEE),
            breathing,
            pumping,
            biting: max_of(Does::BITE),
            pain,
            blood,
        }
    }

    /// Is this body past saving? Breath, cold and hunger are judged elsewhere.
    pub fn fatal(&self, plan: &BodyPlan) -> Option<Cause> {
        for i in 0..plan.parts.len() {
            if !plan.parts[i].does.has(Does::VITAL) {
                continue;
            }
            if self.parts[i].severed {
                // Name the part that was cut off: the head, not the brain.
                let mut top = i;
                while let Some(p) = plan.parts[top].parent {
                    if !self.parts[p].severed {
                        break;
                    }
                    top = p;
                }
                return Some(Cause::Severed(top));
            }
            if self.own(plan, i) <= 0.0 {
                return Some(Cause::Destroyed(i));
            }
        }
        if self.blood < 0.4 * self.max_blood {
            return Some(Cause::BledOut);
        }
        // Burns over much of the body kill.
        let total: f32 = plan.parts.iter().filter(|p| !p.inside).map(|p| p.size).sum();
        let burnt: f32 = (0..plan.parts.len())
            .filter(|&i| !plan.parts[i].inside)
            .map(|i| plan.parts[i].size * self.parts[i].wounds.iter().filter(|w| w.kind == WoundKind::Burn && w.tissue != Tissue::Fur).map(|w| w.severity).fold(0.0, f32::max))
            .sum();
        if burnt > 0.6 * total {
            return Some(Cause::Burned);
        }
        None
    }

    /// Advance `dt` seconds: bleeding (returns the ml lost from open wounds,
    /// which ends up on the floor), clotting and healing. `heal` scales the
    /// healing rate (hunger slows it, sleep speeds it).
    pub fn step(&mut self, plan: &BodyPlan, dt: f32, heal: f32) -> f32 {
        let mut outside = 0.0;
        let mut lost = 0.0;
        // A trickle this small has clotted (scaled to the body: a rat bleeds drops).
        let dry = 0.005 * self.max_blood / 5000.0;
        for (i, s) in self.parts.iter_mut().enumerate() {
            let inside = plan.parts[i].inside;
            let vascular = plan.parts[i].vascular;
            for w in &mut s.wounds {
                if w.bleed > 0.0 {
                    let ml = w.bleed * dt;
                    lost += ml;
                    if !inside && matches!(w.kind, WoundKind::Cut | WoundKind::Puncture) {
                        outside += ml;
                    }
                    // Clotting: the heart's and the great vessels' wounds barely do.
                    let tau = w.tissue.props().clot * vascular;
                    w.bleed *= (-dt / tau).exp();
                    if w.bleed < dry {
                        w.bleed = 0.0;
                    }
                }
                w.age += dt;
                let destroyed = w.tissue == Tissue::Organ && w.severity >= 1.0;
                if !s.severed && !destroyed {
                    w.severity -= heal * heal_rate(w.kind, w.tissue) * dt / 86400.0;
                }
            }
            s.wounds.retain(|w| w.severity > 0.02 || w.bleed > 0.0);
        }
        self.blood -= lost;
        // Blood comes back over about ten days.
        self.blood = (self.blood + heal * self.max_blood * dt / (10.0 * 86400.0)).min(self.max_blood);
        outside
    }

    /// One line per hurt part: its name, its wounds, and a colour for the
    /// worst of them (a bruise's colour shows its age).
    pub fn report(&self, plan: &BodyPlan) -> Vec<(String, Rgb)> {
        let mut out = Vec::new();
        for (i, s) in self.parts.iter().enumerate() {
            // Report a severed limb once, at the cut.
            if s.severed {
                let parent_severed = plan.parts[i].parent.is_some_and(|p| self.parts[p].severed);
                if !parent_severed {
                    out.push((format!("{}: cut off", plan.parts[i].name), Rgb(200, 40, 40)));
                }
                continue;
            }
            if s.wounds.is_empty() {
                continue;
            }
            let words: Vec<String> = s.wounds.iter().map(|w| describe_wound(w, plan.parts[i].inside)).collect();
            let worst = s.wounds.iter().max_by(|a, b| (a.severity * a.tissue.props().weight.max(0.1)).total_cmp(&(b.severity * b.tissue.props().weight.max(0.1)))).unwrap();
            out.push((format!("{}: {}", plan.parts[i].name, words.join(", ")), wound_color(worst)));
        }
        out
    }
}

/// Severity healed per day, at the full rate.
fn heal_rate(kind: WoundKind, tissue: Tissue) -> f32 {
    match (kind, tissue) {
        (WoundKind::Bruise, Tissue::Skin) => 0.15,
        (WoundKind::Bruise, _) => 0.12,
        (WoundKind::Fracture, _) => 0.025,
        (WoundKind::Burn, _) => 0.05,
        (_, Tissue::Skin) => 0.1,
        (_, Tissue::Organ) => 0.04,
        _ => 0.06,
    }
}

/// A bruise goes red, then purple and blue, then green, then yellow, then
/// fades.
pub fn bruise_color(age_secs: f32) -> Rgb {
    const STOPS: [(f32, Rgb); 5] = [
        (0.0, Rgb(205, 64, 72)),
        (1.0, Rgb(130, 56, 150)),
        (3.0, Rgb(78, 70, 170)),
        (5.0, Rgb(120, 160, 70)),
        (8.0, Rgb(205, 190, 80)),
    ];
    let d = age_secs / 86400.0;
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if d <= b {
            return ca.lerp(cb, (d - a) / (b - a));
        }
    }
    STOPS[4].1
}

pub fn wound_color(w: &Wound) -> Rgb {
    match w.kind {
        WoundKind::Bruise if w.tissue == Tissue::Skin => bruise_color(w.age),
        WoundKind::Bruise => Rgb(150, 70, 120),
        WoundKind::Burn => Rgb(230, 130, 50),
        WoundKind::Fracture => Rgb(230, 225, 200),
        _ if w.bleed > 0.0 => Rgb(230, 40, 40),
        _ => Rgb(170, 70, 70),
    }
}

/// "dark bruise", "deep cut (muscle), bleeding", "broken bone".
pub fn describe_wound(w: &Wound, organ: bool) -> String {
    let s = w.severity;
    let mut t = match w.kind {
        WoundKind::Bruise => match w.tissue {
            Tissue::Skin => {
                let d = w.age / 86400.0;
                let age = if d < 1.0 {
                    "fresh"
                } else if d < 4.0 {
                    "dark"
                } else if d < 6.5 {
                    "greenish"
                } else {
                    "yellowing"
                };
                format!("{age} {}bruise", if s > 0.6 { "heavy " } else { "" })
            }
            _ if organ => "bruised".into(),
            _ => "deep bruise".into(),
        },
        WoundKind::Cut | WoundKind::Puncture => {
            if organ && s >= 1.0 {
                "destroyed".into()
            } else {
                let what = match (w.kind, s) {
                    (WoundKind::Cut, s) if s < 0.25 => "nick",
                    (WoundKind::Cut, s) if s < 0.55 => "cut",
                    (WoundKind::Cut, _) => "gash",
                    (_, s) if s < 0.3 => "prick",
                    _ => "puncture",
                };
                if organ {
                    format!("{what}")
                } else {
                    format!("{what} ({})", w.tissue.name())
                }
            }
        }
        WoundKind::Fracture => {
            if s < 0.45 {
                "cracked bone".into()
            } else if s < 0.95 {
                "broken bone".into()
            } else {
                "shattered bone".into()
            }
        }
        WoundKind::Burn => match w.tissue {
            Tissue::Fur => "singed fur".into(),
            _ if s < 0.3 => "light burn".into(),
            _ if s < 0.7 => "burn".into(),
            _ => "deep burn".into(),
        },
    };
    if w.bleed > 0.05 {
        t.push_str(if w.bleed > 2.0 { " POURING BLOOD" } else { " bleeding" });
    }
    t
}
