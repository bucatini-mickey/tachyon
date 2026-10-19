//! Generated languages and the names built from them, Dwarf Fortress style.
//!
//! No real peoples and no real languages: each culture speaks a language
//! generated from a seed. To make them feel like languages (a consistent
//! sound, familiar-ish but unplaceable) each blends the sound systems of one
//! or two "flavours" and mutates the result: dropping some sounds and adding
//! a spelling habit of its own. The flavours are loosely inspired by real
//! phonologies but are only raw material; they are never named in a game,
//! and a blended, mutated language doesn't read as any of them.
//!
//! Every language has a lexicon: English concepts ("salt", "bay", "iron")
//! map to words, the same word every time. Names are built from meaningful
//! words, so every name has a translation: *Varnakkel*, "Saltbay"; Oris
//! *Tlekhaan*, "Iron-Tide".
//!
//! A [`Culture`] is a people with a language: its name for itself, an
//! adjective for English text, given names, and family names with meanings.

use crate::rng::Rng;
use std::collections::HashMap;

/// (sound, weight) lists.
type Weighted = Vec<(String, f32)>;

fn w(s: &str) -> Weighted {
    s.split_whitespace()
        .map(|t| match t.split_once(':') {
            Some((a, b)) => (a.to_string(), b.parse().unwrap_or(3.0)),
            None => (t.to_string(), 3.0),
        })
        .collect()
}

struct Flavour {
    onsets: &'static str,
    nuclei: &'static str,
    codas: &'static str,
    /// Syllable shapes: "CV", "CVC", "V", "CCV".
    shapes: &'static str,
    clusters: &'static str,
    syllables: &'static [usize],
    given_end: &'static str,
    linker: &'static str,
    modifier_first: bool,
}

const FLAVOURS: [Flavour; 12] = [
    // Latin-like
    Flavour { onsets: "p t k:2 c b d g m n l r:4 s:4 v f qu:1 st:1", nuclei: "a:5 e:4 i:4 o:3 u:3 ae:1", codas: "s:4 m:2 n:3 r:2 x:1 t:1", shapes: "CV:5 CVC:3 V:1", clusters: "pr tr cr fl st gr", syllables: &[2, 2, 3, 3], given_end: "us:3 a:3 o:2 ius:1 ex:1", linker: "i", modifier_first: false },
    // Welsh-like
    Flavour { onsets: "ll:2 dd:2 rh:1 ff:1 th:1 g c b d m n:4 w:3 gw:2 t p s", nuclei: "a:4 e:3 y:4 w:2 ai:2 ae:1 o:3 i:2", codas: "n:4 dd:2 th:1 r:3 l:2 ch:2 s:1", shapes: "CV:4 CVC:5 V:1", clusters: "br dr gl gr tr cl", syllables: &[1, 2, 2, 3], given_end: "wyn:2 ydd:2 an:2 og:1 edd:1", linker: "o", modifier_first: false },
    // Esperanto-like
    Flavour { onsets: "b c d f g h j k l:4 m n:4 p r:4 s:4 t v z ŝ:1 ĝ:1", nuclei: "a:5 e:5 i:4 o:5 u:3 aj:1 oj:1", codas: "n:3 s:2 l:2 r:2 j:1", shapes: "CV:6 CVC:3 V:1", clusters: "kr pl tr sp st", syllables: &[2, 2, 3], given_end: "o:4 an:2 io:1 eto:1", linker: "o", modifier_first: true },
    // Slavic-like
    Flavour { onsets: "b v d z zh:2 k l:3 m n p r:4 s t ch:2 sh:2 kh:1 g", nuclei: "a:4 o:4 e:3 i:3 u:2 y:2", codas: "v:2 k:2 n:2 r:2 sk:1 ts:1 d:1 l:1", shapes: "CV:4 CVC:4 CCV:2", clusters: "str zv pr tr gr sv vl br dr", syllables: &[2, 2, 3], given_end: "an:2 or:2 imir:1 ek:2 ota:1", linker: "o", modifier_first: true },
    // Germanic-like
    Flavour { onsets: "b d f g h:2 k l m n r s t th:2 w:2 sk:1 sw:1", nuclei: "a:4 e:4 i:3 o:3 u:3 ei:1 au:1", codas: "d:2 t:2 n:3 r:3 ld:2 nd:2 rk:1 st:1 lf:1 ng:1", shapes: "CV:3 CVC:6", clusters: "br fr gr hr kn st thr", syllables: &[1, 2, 2], given_end: "ric:2 wald:2 mund:1 bert:1 helm:1 olf:1", linker: "e", modifier_first: true },
    // Greek-like
    Flavour { onsets: "p t k:3 th:2 ph:2 ch:2 m n l r:3 s:3 d g x:1 z:1", nuclei: "a:4 e:3 i:3 o:4 y:1 ai:1 eu:1 ou:1", codas: "s:4 n:3 r:1 x:1", shapes: "CV:5 CVC:3 V:1", clusters: "kr pr tr pl kl ps:1", syllables: &[2, 3, 3], given_end: "os:4 as:2 ides:1 eus:1 on:2", linker: "o", modifier_first: true },
    // Nahuatl-like
    Flavour { onsets: "tl:4 tz:2 ch:2 x:2 k:3 qu:2 hu:2 m:3 n:3 p t:3 y:2 z:1", nuclei: "a:5 e:3 i:4 o:4", codas: "tl:3 n:2 k:1 x:1 z:1 h:1", shapes: "CV:6 CVC:3", clusters: "", syllables: &[2, 3, 3], given_end: "tl:3 in:2 atl:1 otl:1 qui:1", linker: "a", modifier_first: true },
    // Finnish-like
    Flavour { onsets: "k:4 t:4 p s:3 h:2 v:3 j:2 l:3 m n:3 r:2", nuclei: "a:4 ä:3 e:3 i:4 o:3 u:3 ö:1 y:2 aa:1 ii:1 uu:1", codas: "n:3 s:2 t:2 l:2 kk:1 tt:1", shapes: "CV:5 CVC:4 V:1", clusters: "", syllables: &[2, 3, 3], given_end: "o:2 i:2 nen:2 la:1 kki:1", linker: "i", modifier_first: true },
    // Swahili-like
    Flavour { onsets: "mb:2 nd:2 ng:2 ny:1 k:3 t:3 m:3 w:3 j:2 s:2 z:1 b:2 l:2 h:1 ch:1", nuclei: "a:5 e:3 i:4 o:3 u:4", codas: "", shapes: "CV:9 V:1", clusters: "", syllables: &[2, 3, 3], given_end: "i:2 e:2 a:2 o:1 ani:1", linker: "a", modifier_first: false },
    // Quechua-like
    Flavour { onsets: "q:3 k:3 ch:2 p t:3 w:3 y:3 ll:2 m n r s:2 h:1", nuclei: "a:6 i:4 u:4", codas: "q:1 k:1 n:2 y:2 s:1 r:1", shapes: "CV:6 CVC:3", clusters: "", syllables: &[2, 2, 3], given_end: "ku:2 q:1 lla:1 y:2 ri:1", linker: "i", modifier_first: true },
    // Japanese-like
    Flavour { onsets: "k:4 s:3 t:3 n:3 h:2 m:3 y:2 r:4 w:1 g:2 z:1 d:1 b:1 sh:2 ch:1 ts:1", nuclei: "a:4 i:4 u:3 e:3 o:4", codas: "n:2", shapes: "CV:8 CVC:1 V:1", clusters: "", syllables: &[2, 3, 3], given_end: "o:2 i:2 ro:1 ki:1 a:1", linker: "", modifier_first: true },
    // Turkish-like
    Flavour { onsets: "b c:1 ç:1 d g h k:3 l m n r:3 s:3 ş:1 t y:2 z:2", nuclei: "a:4 e:4 ı:2 i:3 o:2 ö:1 u:2 ü:1", codas: "n:3 r:3 l:2 k:2 z:1 t:1 ş:1", shapes: "CV:4 CVC:5", clusters: "", syllables: &[2, 2, 3], given_end: "an:2 er:2 ek:1 ay:1 ün:1", linker: "a", modifier_first: true },
];

/// Spelling habits a language can adopt as its own signature.
const SIGNATURES: [&str; 16] = ["kh", "vr", "ss", "aa", "é", "ô", "'", "zh", "dh", "rr", "yr", "ae", "ö", "q", "x", "ll"];

/// Concepts for place names: heads ("bay") and modifiers ("salt").
pub const PLACE_HEADS: [&str; 16] = ["bay", "cape", "isle", "haven", "rock", "hill", "cove", "point", "shore", "harbour", "fort", "town", "reach", "sound", "head", "strand"];
pub const PLACE_MODS: [&str; 30] = ["salt", "stone", "red", "white", "black", "old", "new", "high", "gull", "wind", "sun", "moon", "iron", "grey", "green", "deep", "far", "bright", "olive", "fig", "cedar", "gold", "storm", "dawn", "wolf", "shell", "coral", "king", "saint", "twin"];
/// Concepts for family names.
pub const NAME_WORDS: [&str; 40] = ["iron", "stone", "tide", "wave", "storm", "wind", "oak", "ash", "wolf", "bear", "hawk", "raven", "fire", "salt", "blood", "bone", "hand", "heart", "eye", "shield", "sword", "spear", "hammer", "anchor", "sail", "rope", "star", "moon", "sun", "thunder", "frost", "stern", "bold", "swift", "grey", "red", "black", "gold", "deep", "true"];
/// Concepts for ship names.
pub const SHIP_WORDS: [&str; 30] = ["thunder", "resolve", "valour", "fury", "vengeance", "glory", "triumph", "tempest", "lion", "eagle", "dragon", "serpent", "defiance", "fortune", "victory", "hope", "dawn", "star", "crown", "sword", "shield", "spear", "wave", "wind", "storm", "faith", "honour", "wrath", "courage", "sea"];

const VOWELS: &str = "aeiouyäöü";

fn is_vowel(c: Option<char>) -> bool {
    c.is_some_and(|c| VOWELS.contains(c))
}

/// A stable hash of a string, for seeding per-word randomness.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn pick<'a>(rng: &mut Rng, items: &'a [(String, f32)]) -> &'a str {
    let total: f32 = items.iter().map(|(_, w)| w).sum();
    let mut r = rng.f32() * total;
    for (s, w) in items {
        r -= w;
        if r <= 0.0 {
            return s;
        }
    }
    &items[items.len() - 1].0
}

fn blend(a: &str, b: &str, wb: f32, rng: &mut Rng, drop: f32) -> Weighted {
    let mut merged: Vec<(String, f32)> = Vec::new();
    for (list, k) in [(w(a), 1.0 - wb), (w(b), wb)] {
        for (s, x) in list {
            match merged.iter_mut().find(|(m, _)| *m == s) {
                Some(e) => e.1 += x * k,
                None => merged.push((s, x * k)),
            }
        }
    }
    let kept: Weighted = merged.iter().filter(|(_, x)| rng.f32() > drop || *x > 3.0).cloned().collect();
    if kept.is_empty() { merged } else { kept }
}

#[derive(Clone, Debug)]
pub struct Language {
    pub seed: u64,
    onsets: Weighted,
    nuclei: Weighted,
    codas: Weighted,
    shapes: Weighted,
    clusters: Weighted,
    syllables: Vec<usize>,
    given_end: Weighted,
    linker: String,
    /// "Salt-bay" (true) or "bay-salt" (false).
    pub modifier_first: bool,
    signature: String,
    /// Words fixed by hand (a people's own name, chosen to read well).
    pub fixed: HashMap<String, String>,
}

impl Language {
    /// A language from a seed: one or two flavours blended and mutated.
    pub fn new(seed: u64) -> Language {
        let mut rng = Rng::new(seed.wrapping_mul(2_654_435_761));
        let n = FLAVOURS.len() as u32;
        let a = rng.below(n) as usize;
        let two = rng.chance(0.75);
        let b = if two { (a + 1 + rng.below(n - 1) as usize) % FLAVOURS.len() } else { a };
        let (fa, fb) = (&FLAVOURS[a], &FLAVOURS[b]);
        let wb = if two { 0.2 + rng.f32() * 0.25 } else { 0.0 };
        let drop = 0.1 + rng.f32() * 0.15;
        let either = |rng: &mut Rng| rng.f32() > wb;
        Language {
            seed,
            onsets: blend(fa.onsets, fb.onsets, wb, &mut rng, drop),
            nuclei: blend(fa.nuclei, fb.nuclei, wb, &mut rng, drop * 0.5),
            codas: if fa.codas.is_empty() && fb.codas.is_empty() { Vec::new() } else { blend(fa.codas, fb.codas, wb, &mut rng, drop) },
            shapes: blend(fa.shapes, fb.shapes, wb, &mut rng, 0.0),
            clusters: if fa.clusters.is_empty() && fb.clusters.is_empty() { Vec::new() } else { blend(fa.clusters, fb.clusters, wb, &mut rng, drop) },
            syllables: if either(&mut rng) { fa.syllables.to_vec() } else { fb.syllables.to_vec() },
            given_end: blend(fa.given_end, fb.given_end, wb, &mut rng, drop),
            linker: if either(&mut rng) { fa.linker.into() } else { fb.linker.into() },
            modifier_first: if either(&mut rng) { fa.modifier_first } else { fb.modifier_first },
            signature: if rng.chance(0.7) { SIGNATURES[rng.below(SIGNATURES.len() as u32) as usize].into() } else { String::new() },
            fixed: HashMap::new(),
        }
    }

    fn syllable(&self, rng: &mut Rng) -> String {
        let shape = pick(rng, &self.shapes).to_string();
        let mut s = String::new();
        if shape.starts_with("CC") && !self.clusters.is_empty() {
            s += pick(rng, &self.clusters);
        } else if shape.starts_with('C') {
            s += pick(rng, &self.onsets);
        }
        s += pick(rng, &self.nuclei);
        if shape.ends_with("VC") && !self.codas.is_empty() {
            s += pick(rng, &self.codas);
        }
        s
    }

    /// A new word in this language's sound (not in the lexicon).
    pub fn make_word(&self, rng: &mut Rng, syllables: Option<usize>) -> String {
        let n = syllables.unwrap_or_else(|| *rng.pick(&self.syllables));
        let mut w: String = (0..n.max(1)).map(|_| self.syllable(rng)).collect();
        // The language's signature habit shows up now and then.
        let chars: Vec<char> = w.chars().collect();
        if !self.signature.is_empty() && rng.chance(0.25) && chars.len() > 3 {
            let i = 1 + rng.below(chars.len() as u32 - 2) as usize;
            w = chars[..i].iter().collect::<String>() + &self.signature + &chars[i + 1..].iter().collect::<String>();
        }
        // No three identical letters in a row.
        let mut out: Vec<char> = Vec::new();
        for c in w.chars() {
            let n = out.len();
            if n >= 2 && out[n - 1] == c && out[n - 2] == c {
                continue;
            }
            out.push(c);
        }
        out.into_iter().collect()
    }

    /// This language's word for an English concept (always the same word).
    pub fn word(&self, concept: &str) -> String {
        if let Some(w) = self.fixed.get(concept) {
            return w.clone();
        }
        let mut rng = Rng::new(self.seed ^ hash_str(concept));
        let n = if concept.len() <= 4 && rng.chance(0.6) { 1 } else { *rng.pick(&[1, 2, 2]) };
        self.make_word(&mut rng, Some(n))
    }

    /// (native, translation) for modifier + head, in the language's order:
    /// ("Varnakkel", "Saltbay").
    pub fn compound(&self, modifier: &str, head: &str) -> (String, String) {
        let (a, b) = (self.word(modifier), self.word(head));
        let (mut first, mut second) = if self.modifier_first { (a, b) } else { (b, a) };
        if is_vowel(first.chars().last()) && is_vowel(second.chars().next()) {
            let rest: String = second.chars().skip(1).collect();
            if !rest.is_empty() {
                second = rest;
            }
        } else if !self.linker.is_empty() && !is_vowel(first.chars().last()) && !is_vowel(second.chars().next()) && first.chars().count() + second.chars().count() > 7 {
            first += &self.linker;
        }
        (capitalise(&(first + &second)), format!("{}{}", capitalise(modifier), head))
    }
}

pub fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

const BLOCK: [&str; 17] = ["shit", "cunt", "fuck", "cock", "dick", "piss", "fag", "nig", "tit", "ass", "cum", "poo", "wank", "twat", "slut", "rape", "kill"];
const REAL: [&str; 29] = ["box", "das", "the", "and", "man", "men", "boy", "cat", "dog", "hat", "mad", "sad", "bad", "god", "sin", "war", "was", "his", "her", "him", "yes", "sex", "gay", "fat", "pig", "cow", "rat", "bum", "tit"];

/// Generated words that happen to be real (sometimes rude) English words
/// are rejected as names.
pub fn acceptable(word: &str) -> bool {
    let w = word.to_lowercase();
    !BLOCK.iter().any(|b| w.contains(b)) && !REAL.contains(&w.as_str())
}

/// A people with its own language and naming traditions.
#[derive(Clone, Debug)]
pub struct Culture {
    pub language: Language,
    /// Their name for themselves ("Orvesh"), and the English adjective ("Orveshi").
    pub name: String,
    pub adjective: String,
    pub given: Vec<String>,
    /// Family names, each with its meaning ("Tlekhaan", "Iron-Tide").
    pub family: Vec<(String, String)>,
}

impl Culture {
    pub fn new(seed: u64) -> Culture {
        let mut lang = Language::new(seed);
        let mut rng = Rng::new(seed.wrapping_add(99_991));
        let mut stem = lang.word("people");
        while stem.chars().count() < 4 || !acceptable(&stem) {
            stem = lang.make_word(&mut rng, Some(2));
        }
        lang.fixed.insert("people".into(), stem.clone());
        let name = capitalise(&stem);
        let adjective = if is_vowel(name.chars().last()) {
            name.clone() + *rng.pick(&["n", "ri", "si"])
        } else {
            name.clone() + *rng.pick(&["i", "ian", "ese", "ar", "ic"])
        };
        let mut given: Vec<String> = Vec::new();
        let mut tries = 0;
        while given.len() < 48 && tries < 5000 {
            tries += 1;
            let syl = *rng.pick(&[1, 2, 2]);
            let mut w = lang.make_word(&mut rng, Some(syl));
            if rng.chance(0.5) && !lang.given_end.is_empty() {
                let end = pick(&mut rng, &lang.given_end).to_string();
                if is_vowel(w.chars().last()) && is_vowel(end.chars().next()) {
                    w = w.trim_end_matches(|c| VOWELS.contains(c)).to_string();
                }
                w += &end;
            }
            let w = capitalise(&w);
            let n = w.chars().count();
            if (4..=11).contains(&n) && !given.contains(&w) && acceptable(&w) {
                given.push(w);
            }
        }
        let child = lang.word("son");
        let mut family: Vec<(String, String)> = Vec::new();
        tries = 0;
        while family.len() < 64 && tries < 5000 {
            tries += 1;
            let (native, meaning) = if rng.chance(0.65) {
                let a = *rng.pick(&NAME_WORDS);
                let mut b = *rng.pick(&NAME_WORDS);
                while b == a {
                    b = *rng.pick(&NAME_WORDS);
                }
                (lang.compound(a, b).0, format!("{}-{}", capitalise(a), capitalise(b)))
            } else {
                let g = rng.pick(&given).clone();
                let native = if lang.modifier_first { capitalise(&child) + &g.to_lowercase() } else { g.clone() + &child };
                (native, format!("child of {g}"))
            };
            let n = native.chars().count();
            if (4..=14).contains(&n) && !family.iter().any(|(f, _)| *f == native) && acceptable(&native) {
                family.push((native, meaning));
            }
        }
        Culture { language: lang, name, adjective, given, family }
    }

    /// A given name and a family name.
    pub fn person(&self, rng: &mut Rng) -> (String, String) {
        (rng.pick(&self.given).clone(), rng.pick(&self.family).0.clone())
    }

    /// What a family name means ("Iron-Tide"), if it is one of this people's.
    pub fn meaning(&self, family: &str) -> Option<&str> {
        self.family.iter().find(|(f, _)| f == family).map(|(_, m)| m.as_str())
    }

    /// A place name and its translation ("Varnakkel", "Saltbay").
    pub fn place(&self, rng: &mut Rng) -> (String, String) {
        self.language.compound(rng.pick(&PLACE_MODS), rng.pick(&PLACE_HEADS))
    }

    /// A ship's name: one proud word in the people's tongue, and its meaning.
    pub fn ship(&self, rng: &mut Rng) -> (String, String) {
        let c = *rng.pick(&SHIP_WORDS);
        let w = self.language.word(c);
        if w.chars().count() >= 4 && acceptable(&w) {
            return (capitalise(&w), capitalise(c));
        }
        // Too short to carry on a transom: two proud words instead.
        let mut d = *rng.pick(&SHIP_WORDS);
        while d == c {
            d = *rng.pick(&SHIP_WORDS);
        }
        self.language.compound(d, c)
    }
}
