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
    let lib = parse_library(TLIB).unwrap();
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
    let lib = parse_library(SEED_LIBRARY).unwrap();
    let once = library_text("seed", &lib);
    assert_eq!(library_text("seed", &parse_library(&once).unwrap()), once);
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
    // The strict reader now refuses the line outright (its ink column is the glyph "# .").
    assert!(parse_library(&library_text("t", &[bad.clone()])).is_err(), "this is why the name must be refused");
    assert!(check_name(&bad.name).is_err());

    // Everything the checks allow survives a write and a read.
    let ok = [entry(false, "wall one", '#', ',', &["x", "y"]), entry(true, "mast", '|', '.', &["ship"])];
    let back = parse_library(&library_text("t", &ok)).unwrap();
    for (a, b) in ok.iter().zip(&back) {
        assert_eq!((&a.name, a.glyph, &a.tags), (&b.name, b.glyph, &b.tags));
    }
    assert_eq!(back[0].floor, ',');
}

#[test]
fn a_space_glyph_would_have_slid_the_floor_into_the_wall_slot() {
    let broken = entry(false, "m", ' ', '.', &[]);
    let back = parse_library(&library_text("t", &[broken])).unwrap();
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

// ---- malformed files: a clear error with a line number, no panic, no huge allocation -----------

const GOOD_PART: &str = "tachyon-blueprint 1
name hut
size 3 2 1
key # wall stone block
key . floor stone block
level 0
###
#.#
end
";

fn part_with(tail: &str) -> String {
    format!("{GOOD_PART}{tail}")
}

fn part_err(text: &str) -> String {
    match Part::from_text(text) {
        Err(e) => e,
        Ok(_) => panic!("expected an error for:\n{text}"),
    }
}

#[test]
fn part_socket_problems_name_their_line() {
    // GOOD_PART is 9 lines, so the first socket line is line 10.
    let e = part_err(&part_with("socket 1 0 0\n"));
    assert!(e.starts_with("line 10:") && e.contains("socket needs"), "{e}");
    let e = part_err(&part_with("socket 1 0 0 n door\nsocket 1 0 0 sideways door\n"));
    assert!(e.starts_with("line 11:") && e.contains("unknown direction 'sideways'"), "{e}");
    let e = part_err(&part_with("socket 3 0 0 n door\n"));
    assert!(e.starts_with("line 10:") && e.contains("outside the part"), "{e}");
    let e = part_err(&part_with("socket 0 0 -1 n door\n"));
    assert!(e.contains("outside the part"), "{e}");
    let e = part_err(&part_with("socket x 0 0 n door\n"));
    assert!(e.starts_with("line 10:") && e.contains("bad socket number"), "{e}");
    let e = part_err(&part_with("sockit 1 0 0 n door\n"));
    assert!(e.starts_with("line 10:") && e.contains("unknown line"), "{e}");
    // A tag is optional, and all six directions read.
    let p = Part::from_text(&part_with("socket 1 0 0 down\n# note\nsocket 0 0 0 up x\n")).unwrap();
    assert_eq!(p.sockets.len(), 2);
}

#[test]
fn part_body_problems_name_their_line() {
    let e = part_err("tachyon-blueprint 1\nname x\nsize -1 2 1\nend\n");
    assert!(e.starts_with("line 3:") && e.contains("at least 1"), "{e}");
    let e = part_err("tachyon-blueprint 1\nname x\nsize 0 2 1\nend\n");
    assert!(e.starts_with("line 3:"), "{e}");
    let e = part_err("tachyon-blueprint 1\nname x\nsize 99999 99999 99999\nend\n");
    assert!(e.starts_with("line 3:") && e.contains("too big"), "{e}");
    let e = part_err("tachyon-blueprint 1\nname x\nsize 2147483647 2147483647 2\nend\n");
    assert!(e.contains("too big"), "{e}");
    let e = part_err("tachyon-blueprint 1\nname x\nsize 3 2\nend\n");
    assert!(e.starts_with("line 3:"), "{e}");
    let e = part_err(&GOOD_PART.replace("#.#", "#.#####"));
    assert!(e.starts_with("line 8:") && e.contains("wide"), "{e}");
    let e = part_err(&GOOD_PART.replace("#.#", "#?#"));
    assert!(e.starts_with("line 8:") && e.contains("no key"), "{e}");
    let e = part_err(&GOOD_PART.replace("level 0\n###\n#.#\n", "level 0\n###\n"));
    assert!(e.contains("line"), "{e}");
    let e = part_err(&GOOD_PART.replace("level 0", "level 5"));
    assert!(e.starts_with("line 6:") && e.contains("outside size"), "{e}");
    let e = part_err(&GOOD_PART.replace("key . floor", "key . carpet"));
    assert!(e.starts_with("line 5:") && e.contains("unknown tile"), "{e}");
    let e = part_err(&GOOD_PART.replace("end\n", ""));
    assert!(e.contains("before 'end'"), "{e}");
    let e = part_err("");
    assert!(!e.is_empty());
    let e = part_err("hello\n");
    assert!(e.starts_with("line 1:"), "{e}");
}

#[test]
fn part_garbage_does_not_panic() {
    // Cut a good file at every length, and feed odd bytes: an error or a part, never a panic.
    let text = part_with("socket 1 0 0 n door\n");
    for i in 0..=text.len() {
        if text.is_char_boundary(i) {
            let _ = Part::from_text(&text[..i]);
        }
    }
    for t in ["\0\0\0", "tachyon-blueprint", "tachyon-blueprint 1\nsize", "tachyon-blueprint 1\nkey", "tachyon-blueprint 1\r\nsize 1 1 1\r\nlevel 0\r\n", "\u{feff}tachyon-blueprint 1"] {
        let _ = Part::from_text(t);
    }
}

const GOOD_ART: &str = "tachyon-art 1
name sign
size 2 1
color a 220 180 90
glyphs
T~
ink
aa
paper
aa
end
";

fn art_err(text: &str) -> String {
    match Art::from_text(text) {
        Err(e) => e,
        Ok(_) => panic!("expected an error for:\n{text}"),
    }
}

#[test]
fn art_problems_name_their_line() {
    assert!(Art::from_text(GOOD_ART).is_ok());
    let e = art_err(&GOOD_ART.replace("size 2 1", "size -2 1"));
    assert!(e.starts_with("line 3:"), "{e}");
    let e = art_err(&GOOD_ART.replace("size 2 1", "size 999999 999999"));
    assert!(e.starts_with("line 3:") && e.contains("too big"), "{e}");
    let e = art_err(&GOOD_ART.replace("size 2 1", "size 2"));
    assert!(e.starts_with("line 3:"), "{e}");
    let e = art_err(&GOOD_ART.replace("color a 220 180 90", "color a 220 180"));
    assert!(e.starts_with("line 4:"), "{e}");
    let e = art_err(&GOOD_ART.replace("T~", "T~~~"));
    assert!(e.starts_with("line 6:") && e.contains("wide"), "{e}");
    let e = art_err(&GOOD_ART.replace("aa\npaper", "az\npaper"));
    assert!(e.starts_with("line 8:") && e.contains("no color line"), "{e}");
    let e = art_err(&GOOD_ART.replace("paper\naa\n", "paper\n"));
    assert!(e.contains("ends early") || e.contains("line"), "{e}");
    let e = art_err(&GOOD_ART.replace("end\n", ""));
    assert!(e.contains("before 'end'"), "{e}");
    let e = art_err(&GOOD_ART.replace("name sign", "nmae sign"));
    assert!(e.starts_with("line 2:") && e.contains("unknown line"), "{e}");
    assert!(art_err("nope").starts_with("line 1:"));
    for i in 0..=GOOD_ART.len() {
        let _ = Art::from_text(&GOOD_ART[..i]);
    }
}

#[test]
fn library_problems_name_their_line() {
    let lib = |body: &str| parse_library(&format!("tachyon-library 1\n# c\n{body}\n"));
    assert!(lib("material oak | # + | 150 108 64 | 46 32 18 | wood").is_ok());
    let e = lib("material oak").unwrap_err();
    assert!(e.starts_with("line 3:") && e.contains("no glyph"), "{e}");
    let e = lib("kind bed |  | 1 2 3").unwrap_err();
    assert!(e.starts_with("line 3:") && e.contains("glyph"), "{e}");
    let e = lib("kind bed | BB | 1 2 3").unwrap_err();
    assert!(e.contains("glyph"), "{e}");
    let e = lib("material oak | ### | 1 2 3").unwrap_err();
    assert!(e.contains("glyph"), "{e}");
    let e = lib("kind bed | B | 1 2").unwrap_err();
    assert!(e.contains("ink colour"), "{e}");
    let e = lib("kind bed | B | 1 2 3 | red").unwrap_err();
    assert!(e.contains("paper colour"), "{e}");
    let e = lib("kind bed | B | 1 2 999").unwrap_err();
    assert!(e.contains("ink colour"), "{e}");
    let e = parse_library("kind  | B").unwrap_err();
    assert!(e.starts_with("line 1:") && e.contains("no name"), "{e}");
}

#[test]
fn saving_is_a_temp_file_then_a_rename() {
    let dir = std::env::temp_dir().join(format!("tachyon-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hut.tpart");
    tachyon::library::write_atomic(&path, "first\n").unwrap();
    tachyon::library::write_atomic(&path, "second\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
    assert!(!dir.join("hut.tpart.tmp").exists(), "the temp file is gone after the rename");
    // A failed save (the folder is missing) leaves nothing behind and reports an error.
    assert!(tachyon::library::write_atomic(&dir.join("nope").join("x.tpart"), "x").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
