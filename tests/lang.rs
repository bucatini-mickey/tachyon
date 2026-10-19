//! Generated languages and names.

use tachyon::lang::{acceptable, Culture, Language};
use tachyon::rng::Rng;

#[test]
fn a_seed_always_gives_the_same_people() {
    let (a, b) = (Culture::new(42), Culture::new(42));
    assert_eq!(a.name, b.name);
    assert_eq!(a.given, b.given);
    assert_eq!(a.family, b.family);
    assert_eq!(a.language.word("salt"), b.language.word("salt"));
    assert_ne!(Culture::new(43).name, a.name, "another seed, another people");
}

#[test]
fn words_are_stable_and_compounds_translate() {
    let l = Language::new(7);
    assert_eq!(l.word("iron"), l.word("iron"));
    let (native, meaning) = l.compound("salt", "bay");
    assert_eq!(meaning, "Saltbay");
    assert!(native.chars().next().unwrap().is_uppercase());
    assert!(native.chars().count() >= 3);
}

#[test]
fn cultures_have_plenty_of_clean_names() {
    for seed in 0..20 {
        let c = Culture::new(seed);
        assert_eq!(c.given.len(), 48, "seed {seed}");
        assert_eq!(c.family.len(), 64, "seed {seed}");
        assert!(c.given.iter().all(|g| acceptable(g) && (4..=11).contains(&g.chars().count())));
        assert!(c.family.iter().all(|(f, m)| acceptable(f) && !m.is_empty()));
        assert!(c.adjective.starts_with(&c.name));
    }
}

#[test]
fn print_a_few_peoples() {
    for seed in [1u64, 2, 3, 4] {
        let c = Culture::new(seed);
        let mut rng = Rng::new(seed);
        let people: Vec<String> = (0..4).map(|_| {
            let (g, f) = c.person(&mut rng);
            format!("{g} {f} ({})", c.meaning(&f).unwrap_or(""))
        }).collect();
        let (place, means) = c.place(&mut rng);
        let (ship, ship_means) = c.ship(&mut rng);
        println!("{} ({}): {} | {place} \"{means}\" | the {ship} \"{ship_means}\"", c.name, c.adjective, people.join(", "));
    }
}
