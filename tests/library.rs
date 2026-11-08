//! The Tokamak file formats (`.tlib`, `.tpart`, `.tart`) load and write back byte for byte.

use tachyon::library::{library_text, parse_library, Art, Part, SEED_LIBRARY};

const TLIB: &str = "tachyon-library 1
# The sample library: materials (wall and floor glyphs) and kinds (things).
material stone block | # . | 178 176 168 | 52 50 48 | stone masonry
material water | ~ ~ | 96 150 196 | 16 38 64 | liquid
kind mast | | | 200 170 110 | 0 0 0 | ship
kind door | ' | 200 150 90 | 0 0 0 | opening
";

const TPART: &str = "tachyon-blueprint 1
name hut
size 3 2 1
key # wall stone block
key . floor stone block
level 0
###
#.#
end
socket 1 0 0 n door
socket 2 1 0 down curtain-wall
";

const TART: &str = "tachyon-art 1
name sign
size 2 1
color a 220 180 90
color b 0 0 0
color c 110 170 220
glyphs
T~
ink
ac
paper
bb
end
";

#[test]
fn library_file_round_trips_byte_for_byte() {
    let lib = parse_library(TLIB);
    assert_eq!(lib.len(), 4);
    assert_eq!(library_text("sample", &lib), TLIB);
}

#[test]
fn part_file_round_trips_byte_for_byte() {
    let p = Part::from_text(TPART).unwrap();
    assert_eq!((p.bp.w, p.bp.h, p.bp.d, p.sockets.len()), (3, 2, 1, 2));
    assert_eq!(p.text(), TPART);
}

#[test]
fn art_file_round_trips_byte_for_byte() {
    let a = Art::from_text(TART).unwrap();
    assert_eq!((a.w, a.h, a.cells[1].ch), (2, 1, '~'));
    assert_eq!(a.text(), TART);
}

#[test]
fn the_seed_library_parses_and_rewrites_stably() {
    let lib = parse_library(SEED_LIBRARY);
    let once = library_text("seed", &lib);
    assert_eq!(library_text("seed", &parse_library(&once)), once);
}

// ---- typed text must read back the same, and names must be safe file names ----

use tachyon::library::{check_glyph, check_name, check_tag, clean_file_name, Entry, DEFAULT_BG, DEFAULT_INK};

fn entry(kind: bool, name: &str, glyph: char, floor: char, tags: &[&str]) -> Entry {
    Entry { kind, name: name.into(), glyph, floor, fg: DEFAULT_INK, bg: DEFAULT_BG, tags: tags.iter().map(|t| t.to_string()).collect() }
}

#[test]
fn a_bar_in_a_name_is_refused() {
    assert!(check_name("a | b").is_err());
    assert!(check_name("a|b").is_err());
    assert!(check_name("").is_err());
    assert!(check_name("   ").is_err());
    // Spaces stay legal: the starter set has "stone block".
    assert!(check_name("stone block").is_ok());
}

#[test]
fn a_bar_or_space_in_a_tag_is_refused() {
    assert!(check_tag("a|b").is_err());
    assert!(check_tag("a b").is_err());
    assert!(check_tag("metal").is_ok());
}

#[test]
fn a_space_glyph_is_refused_and_a_floor_bar_too() {
    assert!(check_glyph(' ', false).is_err());
    assert!(check_glyph(' ', true).is_err());
    assert!(check_glyph('|', true).is_err());
    assert!(check_glyph('|', false).is_ok()); // a mast
    assert!(check_glyph('#', false).is_ok());
}

#[test]
fn the_name_that_broke_reload_is_now_refused_and_what_is_allowed_reads_back() {
    // Before: a rename to "a | b" was saved as "material a | b | # . | ..." and read back
    // as a material "a" with glyph "b". The check stops that name getting in.
    let bad = entry(false, "a | b", '#', '.', &[]);
    let back = parse_library(&library_text("t", &[bad.clone()]));
    assert_ne!(back[0].name, bad.name, "this is why the name must be refused");
    assert!(check_name(&bad.name).is_err());

    // Everything the checks allow survives a write and a read.
    let ok = [entry(false, "wall one", '#', ',', &["x", "y"]), entry(true, "mast", '|', '.', &["ship"])];
    let back = parse_library(&library_text("t", &ok));
    for (a, b) in ok.iter().zip(&back) {
        assert_eq!((&a.name, a.glyph, &a.tags), (&b.name, b.glyph, &b.tags));
    }
    assert_eq!(back[0].floor, ',');
}

#[test]
fn a_space_glyph_would_have_slid_the_floor_into_the_wall_slot() {
    let broken = entry(false, "m", ' ', '.', &[]);
    let back = parse_library(&library_text("t", &[broken]));
    assert_eq!(back[0].glyph, '.', "why a space glyph is refused");
}

#[test]
fn file_names_lose_slashes_and_dots_that_climb() {
    for raw in ["..\\x", "../x", "a/b", "a\\b", "..", "...", "a:b", "x*y?", "<q>"] {
        let (clean, changed) = clean_file_name(raw);
        assert!(changed, "{raw}");
        assert!(!clean.contains(['/', '\\', ':', '*', '?', '<', '>', '|']), "{raw} -> {clean}");
        assert!(!clean.contains(".."), "{raw} -> {clean}");
        assert!(!clean.starts_with('.') && !clean.is_empty(), "{raw} -> {clean}");
    }
    assert_eq!(clean_file_name("..\\x").0, "x");
    assert_eq!(clean_file_name("a/b").0, "a-b");
    assert_eq!(clean_file_name("my project").0, "my-project");
}

#[test]
fn windows_reserved_names_are_changed() {
    for raw in ["con", "NUL", "aux", "com1", "LPT9", "con.txt", "Prn"] {
        let (clean, changed) = clean_file_name(raw);
        assert!(changed, "{raw}");
        assert_eq!(clean, format!("_{raw}"));
    }
    // Near misses are fine.
    assert_eq!(clean_file_name("console"), ("console".to_string(), false));
    assert_eq!(clean_file_name("com10"), ("com10".to_string(), false));
}

#[test]
fn a_good_file_name_is_left_alone() {
    assert_eq!(clean_file_name("tower-2"), ("tower-2".to_string(), false));
    assert_eq!(clean_file_name("v1.2"), ("v1.2".to_string(), false));
}
